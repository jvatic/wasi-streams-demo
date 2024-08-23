use ::wasi as wasi_bindgen;
use anyhow::Context;
use exports::test::example::guest_contract::{self, Guest, Message};
use futures_lite::{AsyncBufReadExt, AsyncWriteExt, StreamExt};
use test::example::host_contract;
use wasi_async_runtime::{
    block_on,
    streams::{InputStream, OutputStream},
};

wit_bindgen::generate!({
    path: "../wit",
    world: "my-world",
    with: {
        "wasi:io/error@0.2.1": wasi_bindgen::io::error,
        "wasi:io/poll@0.2.1": wasi_bindgen::io::poll,
        "wasi:io/streams@0.2.1": wasi_bindgen::io::streams,
    },
    additional_derives: [serde::Deserialize]
});

export!(MyWorld);

struct MyWorld;

impl Guest for MyWorld {
    fn input_output_stream(
        inputs: exports::test::example::guest_contract::InputStream,
        outputs: exports::test::example::guest_contract::OutputStream,
    ) -> Result<i64, String> {
        let mut num = host_contract::Arithmetic::new(0);
        let res: anyhow::Result<i64> = block_on(|r| async move {
            let mut ops = Vec::new();

            let inputs = InputStream::new(inputs, r.clone());
            let mut outputs = OutputStream::new(outputs, r);

            let mut lines = inputs.lines();
            while let Some(line) = lines
                .next()
                .await
                .transpose()
                .context("error reading input")?
            {
                let message: Message =
                    serde_json::from_str(&line).context("invalid message: {line:?}")?;

                if let Message::PrintResult = message {
                    let line = format_result(&num, &ops);
                    ops.clear();
                    ops.push(format!("{}", num.get()));

                    outputs
                        .write_all(line.as_bytes())
                        .await
                        .context("error writing output")?;
                    outputs
                        .write("\n".as_bytes())
                        .await
                        .context("error writing output")?;
                    outputs.flush().await.context("error flushing output")?;
                } else {
                    num = process_message(&mut ops, num, message);
                }
            }
            // write an empty line to signal the end of the stream
            outputs
                .write("\n".as_bytes())
                .await
                .context("error writing output")?;
            outputs.flush().await.context("error flushing output")?;
            Ok(num.get())
        });
        res.map_err(|err| format!("error processing stream: {err:?}"))
    }

    fn process_messages(messages: Vec<guest_contract::Message>) -> i64 {
        let mut ops = Vec::new();
        messages
            .into_iter()
            .fold(host_contract::Arithmetic::new(0), |num, message| {
                process_message(&mut ops, num, message)
            })
            .get()
    }
}

fn process_message(
    ops: &mut Vec<String>,
    num: host_contract::Arithmetic,
    message: Message,
) -> host_contract::Arithmetic {
    match message {
        Message::Initialize(n) => {
            ops.clear();
            ops.push(format!("{n}"));
            host_contract::Arithmetic::new(n)
        }
        Message::Add(n) => {
            ops.push(format!("+ {n}"));
            num.add(n);
            num
        }
        Message::Sub(n) => {
            ops.push(format!("- {n}"));
            num.sub(n);
            num
        }
        Message::PrintResult => {
            eprintln!("guest: {}", format_result(&num, &ops));
            ops.clear();
            ops.push(format!("{}", num.get()));
            num
        }
    }
}

fn format_result(num: &host_contract::Arithmetic, ops: &Vec<String>) -> String {
    if ops.is_empty() {
        return "<empty buffer>".to_owned();
    }
    format!("{} = {}", ops.join(" "), num.get())
}

// not used
fn main() {}
