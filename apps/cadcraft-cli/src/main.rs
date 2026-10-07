//! `cadcraft-cli`: headless CADCraft.
//!
//! ```text
//! cadcraft-cli info FILE.dxf                       summary as JSON
//! cadcraft-cli convert IN.dxf OUT.(dxf|svg|png)    convert / export
//! cadcraft-cli run [FILE|--sample|--metric] [--script TEXT|--script-file F.scr] [--cmd 'id {json}']... [--save OUT] [--export OUT.png]
//! cadcraft-cli commands [FILTER]                   command catalog (JSON)
//! cadcraft-cli mcp [--connect HOST:PORT]           MCP server on stdio (headless or bridged to the app)
//! ```
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

use std::process::ExitCode;

use cadcraft_engine::Session;
use serde_json::{Value, json};

const USAGE: &str = "usage:
  cadcraft-cli info FILE.dxf
  cadcraft-cli convert IN.dxf OUT.(dxf|svg|png)
  cadcraft-cli run [FILE | --sample | --metric] [--script TEXT] [--script-file F.scr] [--cmd 'id {json}']... [--save OUT.dxf] [--export OUT.(png|svg)]
  cadcraft-cli commands [FILTER]
  cadcraft-cli mcp [--connect HOST:PORT]
  cadcraft-cli --version";

fn install_io() {
    cadcraft_engine::cmd::file::set_io(cadcraft_engine::cmd::file::IoHooks {
        read: |b, name| cadcraft_io::read(b, name).map_err(|e| e.to_string()),
        write: |d, name| cadcraft_io::write(d, name).map_err(|e| e.to_string()),
    });
}

fn open(s: &mut Session, path: &str) -> Result<(), String> {
    s.execute("open", &json!({ "path": path })).map(|_| ()).map_err(|e| e.to_string())
}

fn info(path: &str) -> Result<(), String> {
    let mut s = Session::empty();
    open(&mut s, path)?;
    let v = s.execute("drawing.inspect", &json!({ "entities": false })).map_err(|e| e.to_string())?;
    println!("{}", serde_json::to_string_pretty(&v).unwrap_or_default());
    Ok(())
}

fn export(s: &Session, out: &str) -> Result<(), String> {
    let d = s.doc().map_err(|e| e.to_string())?;
    let bytes = cadcraft_io::write(d, out).map_err(|e| e.to_string())?;
    std::fs::write(out, &bytes).map_err(|e| format!("{out}: {e}"))?;
    eprintln!("wrote {out} ({} bytes)", bytes.len());
    Ok(())
}

fn run(args: &[String]) -> Result<(), String> {
    let mut s = Session::empty();
    let mut it = args.iter();
    let mut save = None;
    let mut exp = None;
    let mut steps: Vec<(String, String)> = Vec::new();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--sample" => {
                s.open_drawing(cadcraft_engine::sample::default_sample(), "Bracket", None);
            }
            "--metric" => {
                s.new_drawing(true);
            }
            "--script" => steps.push(("script".into(), it.next().ok_or("--script needs text")?.replace("\\n", "\n"))),
            "--script-file" => {
                let p = it.next().ok_or("--script-file needs a path")?;
                steps.push(("script".into(), std::fs::read_to_string(p).map_err(|e| format!("{p}: {e}"))?));
            }
            "--cmd" => steps.push(("cmd".into(), it.next().ok_or("--cmd needs `id {json}`")?.clone())),
            "--save" => save = Some(it.next().ok_or("--save needs a path")?.clone()),
            "--export" => exp = Some(it.next().ok_or("--export needs a path")?.clone()),
            f if !f.starts_with("--") => open(&mut s, f)?,
            other => return Err(format!("unknown option {other}")),
        }
    }
    if s.docs.is_empty() {
        s.new_drawing(false);
    }
    for (kind, body) in steps {
        match kind.as_str() {
            "script" => s.script(&body).map_err(|e| e.to_string())?,
            _ => {
                let (id, json) = body.split_once(' ').unwrap_or((&body, "{}"));
                let params: Value = serde_json::from_str(json).map_err(|e| format!("--cmd {id}: {e}"))?;
                let r = s.execute(id, &params).map_err(|e| e.to_string())?;
                if !r.is_null() {
                    println!("{r}");
                }
            }
        }
    }
    for l in &s.log {
        eprintln!("{l}");
    }
    if let Some(p) = save {
        export(&s, &p)?;
    }
    if let Some(p) = exp {
        export(&s, &p)?;
    }
    Ok(())
}

fn main() -> ExitCode {
    install_io();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let rest: Vec<String> = args.iter().skip(1).cloned().collect();
    let r = match args.first().map(String::as_str) {
        Some("info") => rest.first().ok_or_else(|| USAGE.to_string()).and_then(|p| info(p)),
        Some("convert") => match rest.as_slice() {
            [i, o] => {
                let mut s = Session::empty();
                open(&mut s, i).and_then(|_| export(&s, o))
            }
            _ => Err(USAGE.into()),
        },
        Some("run") => run(&rest),
        Some("commands") => {
            let s = Session::new();
            let f = rest.first().map(|x| x.to_ascii_lowercase());
            let v: Vec<Value> = cadcraft_engine::command_specs()
                .iter()
                .filter(|c| f.as_ref().is_none_or(|f| c.id.contains(f.as_str()) || c.label.to_ascii_lowercase().contains(f.as_str())))
                .map(|c| serde_json::to_value(c.info(&s)).unwrap_or_default())
                .collect();
            println!("{}", serde_json::to_string_pretty(&v).unwrap_or_default());
            Ok(())
        }
        Some("mcp") => {
            let backend: Box<dyn cadcraft_mcp::Backend> = match rest.iter().position(|a| a == "--connect").and_then(|i| rest.get(i + 1)) {
                Some(addr) => match cadcraft_mcp::Remote::connect(addr) {
                    Ok(r) => Box::new(r),
                    Err(e) => {
                        eprintln!("cadcraft-cli: cannot connect to {addr}: {e}");
                        return ExitCode::FAILURE;
                    }
                },
                None => Box::new(cadcraft_mcp::Headless::default()),
            };
            let stdin = std::io::stdin();
            let stdout = std::io::stdout();
            cadcraft_mcp::Server::new(backend).serve(stdin.lock(), stdout.lock()).map_err(|e| e.to_string())
        }
        Some("--version" | "-V") => {
            println!("cadcraft-cli {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        _ => Err(USAGE.into()),
    };
    match r {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}
