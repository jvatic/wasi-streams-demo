use std::{env::args, time::Duration};

use anyhow::{Context, Result};

use exports::test::example::guest_contract::Message;
use test::example::host_contract::{self, Host, HostArithmetic};
use tokio::io::{simplex, AsyncBufReadExt, AsyncWriteExt, BufReader};
use wasi_preview1_component_adapter_provider::{
    WASI_SNAPSHOT_PREVIEW1_ADAPTER_NAME, WASI_SNAPSHOT_PREVIEW1_REACTOR_ADAPTER,
};
use wasmtime::{
    self,
    component::{Component, Linker, Resource},
    Config, Engine, Store,
};
use wasmtime_wasi::{
    self,
    pipe::{AsyncReadStream, AsyncWriteStream},
    AsyncStdoutStream, InputStream, OutputStream, ResourceTable, WasiCtx, WasiCtxBuilder, WasiView,
};

wasmtime::component::bindgen!({
    path: "../wit",
    world: "my-world",
    async: true,
    with: {
        "wasi": wasmtime_wasi::bindings,
        "test:example/host-contract/arithmetic": Arithmetic,
    },
    additional_derives: [serde::Serialize]
});

struct HostState {
    table: ResourceTable,
    wasi: WasiCtx,
}

impl Default for HostState {
    fn default() -> Self {
        let table = ResourceTable::new();
        let wasi = WasiCtxBuilder::new().inherit_stderr().build();
        Self { table, wasi }
    }
}

#[wasmtime_wasi::async_trait]
impl Host for HostState {}

pub struct Arithmetic(i64);

impl Arithmetic {
    fn add(&mut self, other: i64) -> Option<i64> {
        self.0 = self.0.checked_add(other)?;
        Some(self.0)
    }

    fn sub(&mut self, other: i64) -> Option<i64> {
        self.0 = self.0.checked_sub(other)?;
        Some(self.0)
    }
}

#[wasmtime_wasi::async_trait]
impl HostArithmetic for HostState {
    async fn new(&mut self, value: i64) -> Resource<Arithmetic> {
        self.table.push(Arithmetic(value)).unwrap()
    }

    async fn add(&mut self, res: Resource<Arithmetic>, other: i64) -> Option<i64> {
        self.table.get_mut(&res).map(|o| o.add(other)).unwrap()
    }

    async fn sub(&mut self, res: Resource<Arithmetic>, other: i64) -> Option<i64> {
        self.table.get_mut(&res).map(|o| o.sub(other)).unwrap()
    }

    async fn get(&mut self, res: Resource<Arithmetic>) -> i64 {
        self.table.get(&res).map(|o| o.0).unwrap()
    }

    fn drop(&mut self, res: Resource<Arithmetic>) -> wasmtime::Result<()> {
        Ok(self.table.delete(res).map(|_| {})?)
    }
}

impl WasiView for HostState {
    fn table(&mut self) -> &mut ResourceTable {
        &mut self.table
    }
    fn ctx(&mut self) -> &mut WasiCtx {
        &mut self.wasi
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let mut config = Config::new();
    config.wasm_component_model(true).async_support(true);

    let engine = Engine::new(&config)?;

    let mut host_state = HostState::default();
    let table = host_state.table();

    let (host_rx, guest_tx) = simplex(10);
    let (guest_rx, mut host_tx) = simplex(10);
    let host_rx_task = tokio::spawn(async move {
        let mut lines = vec![
            Message::Initialize(42),
            Message::Add(12),
            Message::Sub(14),
            Message::Sub(6),
            Message::PrintResult,
            Message::Add(42),
            Message::PrintResult,
            Message::Sub(4),
            Message::PrintResult,
        ]
        .into_iter();

        while let Some(line) = lines.next() {
            host_tx.write_all(&serde_json::to_vec(&line)?).await?;
            host_tx.write("\n".as_bytes()).await?;
            host_tx.flush().await?;
        }

        host_tx.shutdown().await?;

        Ok::<_, anyhow::Error>(())
    });

    let host_tx_task = tokio::spawn(async move {
        let reader = BufReader::new(host_rx);
        let mut lines = reader.lines();
        while let Some(line) = lines.next_line().await? {
            if line == "" {
                break;
            }
            eprintln!("host: received line from guest: {line}");
        }

        Ok::<_, anyhow::Error>(())
    });

    let input_stream = InputStream::Host(Box::new(AsyncReadStream::new(guest_rx)));
    let input_stream = table.push(input_stream)?;
    let output_stream: OutputStream =
        Box::new(AsyncStdoutStream::new(AsyncWriteStream::new(64, guest_tx)));
    let output_stream = table.push(output_stream)?;

    let mut store = Store::new(&engine, host_state);

    let args: Vec<String> = args().collect();
    let wasm = tokio::fs::read(
        args.get(1)
            .expect("must provide path to wasm binary as the first argument"),
    )
    .await?;
    let wasm = wit_component::ComponentEncoder::default()
        .module(&wasm)?
        .adapter(
            WASI_SNAPSHOT_PREVIEW1_ADAPTER_NAME,
            WASI_SNAPSHOT_PREVIEW1_REACTOR_ADAPTER,
        )?
        .validate(true)
        .encode()?;

    let component = Component::from_binary(&engine, &wasm)?;

    let mut linker = Linker::new(&engine);
    host_contract::add_to_linker(&mut linker, |s| s)?;
    wasmtime_wasi::add_to_linker_async(&mut linker)?;

    let guest = MyWorld::instantiate_async(&mut store, &component, &linker).await?;
    let guest = guest.test_example_guest_contract();
    let res = guest
        .call_process_messages(
            &mut store,
            &[
                Message::PrintResult,
                Message::Initialize(14),
                Message::Add(14),
                Message::Add(14),
                Message::PrintResult,
                Message::Sub(72),
                Message::Sub(12),
                Message::PrintResult,
            ],
        )
        .await
        .context("call_process_messages")?;
    eprintln!("host: First result is {res}");
    match guest
        .call_input_output_stream(&mut store, input_stream, output_stream)
        .await
        .context("call_input_output_stream")?
    {
        Ok(num) => eprintln!("host: Second result is {num}"),
        Err(err) => eprintln!("guest returned an error: {err:?}"),
    };

    host_rx_task.await??;
    host_tx_task.await??;

    eprintln!("All done!");

    Ok(())
}
