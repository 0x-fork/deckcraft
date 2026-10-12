//! Headless DeckCraft. `deckcraft-cli help` explains each subcommand (`help.rs`).
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![forbid(unsafe_code)]

mod help;

use std::process::ExitCode;

use deckcraft_engine::Session;
use serde_json::{Value, json};

macro_rules! outln {
    ($($arg:tt)*) => {{
        use std::io::Write as _;
        if let Err(e) = writeln!(std::io::stdout(), $($arg)*) {
            if e.kind() == std::io::ErrorKind::BrokenPipe {
                std::process::exit(0);
            }
            eprintln!("deckcraft-cli: can't write to stdout: {e}");
            std::process::exit(1);
        }
    }};
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let rest = args.get(1..).unwrap_or(&[]);
    let asks_help = rest.iter().any(|a| a == "--help" || a == "-h");
    if let Some(page) = args.first().filter(|_| asks_help).and_then(|n| help::page(n)) {
        outln!("{page}");
        return ExitCode::SUCCESS;
    }
    match args.first().map(String::as_str) {
        Some("help" | "--help" | "-h") => match rest.first() {
            None => {
                outln!("{}", help::overview());
                ExitCode::SUCCESS
            }
            Some(name) => match help::page(name) {
                Some(page) => {
                    outln!("{page}");
                    ExitCode::SUCCESS
                }
                None => unknown(name),
            },
        },
        Some("--version" | "-V" | "version") => {
            outln!("deckcraft-cli {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Some("info") => report(info(rest)),
        Some("render") => report(render(rest)),
        Some("convert") => report(convert(rest)),
        Some("run") => report(run(rest)),
        Some("commands") => {
            let f = rest.first().map(|x| x.to_lowercase());
            for c in deckcraft_engine::command_specs()
                .iter()
                .filter(|c| f.as_ref().is_none_or(|f| c.id.to_lowercase().contains(f.as_str()) || c.label.to_lowercase().contains(f.as_str())))
            {
                match c.params.strip_prefix("{}") {
                    Some(answer) => outln!("{} — {}{answer}", c.id, c.label),
                    None => outln!("{} — {}: {}", c.id, c.label, c.params),
                }
            }
            ExitCode::SUCCESS
        }
        Some("describe") => report(describe(rest.first().map(String::as_str))),
        Some("app") => report(app(rest)),
        Some("mcp") => report(mcp(rest)),
        Some("links") => {
            use deckcraft_engine::links::*;
            outln!("Discord   {DISCORD}\nWebsite   {WEBSITE}\nApp page  {APP_PAGE}\nGitHub    {GITHUB}");
            ExitCode::SUCCESS
        }
        Some(other) => unknown(other),
        None => usage(),
    }
}

fn unknown(sub: &str) -> ExitCode {
    eprintln!("deckcraft-cli: unknown subcommand `{sub}`\n");
    usage()
}

fn usage() -> ExitCode {
    eprintln!("{}", help::overview());
    eprintln!("\nCommunity: {}  ·  {}", deckcraft_engine::links::DISCORD, deckcraft_engine::links::APP_PAGE);
    ExitCode::FAILURE
}

fn report(r: Result<(), String>) -> ExitCode {
    match r {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("deckcraft-cli: {e}");
            ExitCode::FAILURE
        }
    }
}

fn open(s: &mut Session, input: Option<&str>, sample: bool) -> Result<(), String> {
    if sample {
        return deckcraft_engine::sample::open_sample(s).map_err(|e| e.to_string());
    }
    match input {
        Some(p) => s.execute("file.open", &json!({"path": p})).map(|_| ()).map_err(|e| e.to_string()),
        None => s.execute("file.new", &json!({})).map(|_| ()).map_err(|e| e.to_string()),
    }
}

fn info(args: &[String]) -> Result<(), String> {
    let path = args.first().ok_or("info needs a FILE")?;
    let mut s = Session::new();
    open(&mut s, Some(path), false)?;
    let v = s.execute("document.inspect", &json!({})).map_err(|e| e.to_string())?;
    outln!("{}", serde_json::to_string_pretty(&v).unwrap_or_default());
    Ok(())
}

fn render(args: &[String]) -> Result<(), String> {
    let mut input = None;
    let mut sample = false;
    let mut slide: Option<usize> = None;
    let mut all = false;
    let mut scale = 2.0;
    let mut edit = false;
    let mut out = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--sample" => sample = true,
            "--slide" => slide = Some(it.next().and_then(|v| v.parse().ok()).ok_or("--slide needs a number")?),
            "--all" => all = true,
            "--edit" => edit = true,
            "--scale" => scale = it.next().and_then(|v| v.parse().ok()).ok_or("--scale needs a number")?,
            other
                if input.is_none()
                    && !sample
                    && !other.starts_with("--")
                    && out.is_none()
                    && !(other.ends_with(".png") || other.ends_with(".jpg"))
                    || (input.is_none() && !sample && std::path::Path::new(other).is_file()) =>
            {
                input = Some(other.to_string())
            }
            other => out = Some(other.to_string()),
        }
    }
    let out = out.ok_or("render needs an output path")?;
    let mut s = Session::new();
    open(&mut s, input.as_deref(), sample)?;
    let st = s.doc().map_err(|e| e.to_string())?;
    let n = st.doc.slides.len();
    let list: Vec<usize> = if all { (0..n).collect() } else { vec![slide.unwrap_or(0)] };
    if all {
        std::fs::create_dir_all(&out).map_err(|e| format!("{out}: {e}"))?;
    }
    for i in list {
        if i >= n {
            return Err(format!("no slide {i} (the deck has {n})"));
        }
        let (png, w, h) = deckcraft_engine::cmd::file::render_png(&st.doc, i, scale, edit);
        let path = if all { format!("{out}/slide-{:02}.png", i + 1) } else { out.clone() };
        std::fs::write(&path, png).map_err(|e| format!("{path}: {e}"))?;
        outln!("{path} ({w}×{h})");
    }
    Ok(())
}

fn convert(args: &[String]) -> Result<(), String> {
    let (Some(input), Some(out)) = (args.first(), args.get(1)) else { return Err("convert needs IN and OUT".into()) };
    let mut s = Session::new();
    open(&mut s, Some(input), false)?;
    let fmt = deckcraft_engine::cmd::file::format_for_path(out);
    let r = if matches!(fmt, "png" | "jpeg") {
        s.execute("file.export", &json!({"path": out, "slide": 0}))
    } else {
        s.execute("file.saveAs", &json!({"path": out}))
    };
    r.map_err(|e| e.to_string())?;
    outln!("{out}");
    Ok(())
}

fn run(args: &[String]) -> Result<(), String> {
    let mut input = None;
    let mut sample = false;
    let mut cmds = vec![];
    let mut save = None;
    let mut export = None;
    let mut print = false;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--in" => input = Some(it.next().cloned().ok_or("--in needs a file")?),
            "--sample" => sample = true,
            "--cmd" => cmds.push(it.next().cloned().ok_or("--cmd needs ID[=JSON]")?),
            "--save" => save = Some(it.next().cloned().ok_or("--save needs a path")?),
            "--export" => export = Some(it.next().cloned().ok_or("--export needs a path")?),
            "--print" => print = true,
            other => return Err(format!("unknown option `{other}`")),
        }
    }
    let mut s = Session::new();
    open(&mut s, input.as_deref(), sample)?;
    for c in cmds {
        let (id, p) = match c.split_once('=') {
            Some((id, j)) => (id.to_string(), serde_json::from_str::<Value>(j).map_err(|e| format!("{id}: bad JSON: {e}"))?),
            None => (c.clone(), json!({})),
        };
        let r = s.execute(&id, &p).map_err(|e| e.to_string())?;
        if print {
            outln!("{id} → {r}");
        }
    }
    if let Some(p) = save {
        s.execute("file.saveAs", &json!({"path": p})).map_err(|e| e.to_string())?;
    }
    if let Some(p) = export {
        s.execute("file.export", &json!({"path": p})).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn describe(id: Option<&str>) -> Result<(), String> {
    let id = id.ok_or("describe needs a command id")?;
    let c = deckcraft_engine::find_command(id).ok_or_else(|| deckcraft_engine::EngineError::UnknownCommand(id.into()).to_string())?;
    let mut head = format!("{}: {}", c.id, c.label);
    if !c.menu.is_empty() {
        head.push_str(&format!(", in {}", c.menu.join(" ▸ ")));
    }
    if let Some(k) = c.shortcut {
        head.push_str(&format!(", shortcut {k}"));
    }
    outln!("{head}.\n");
    let (args, answer) = c.params.split_once('→').unwrap_or((c.params, ""));
    let parsed = deckcraft_engine::cmd::parse_params(c.params);
    match &parsed {
        Err(_) => outln!("Parameters: {}", args.trim()),
        Ok(params) if params.is_empty() => outln!("Takes no parameters."),
        Ok(params) => print_params(params),
    }
    let params = parsed.unwrap_or_default();
    if let Some(acts) = c.acts_on() {
        outln!("{acts}");
    }
    let remark = remark(args);
    if !remark.is_empty() {
        outln!("Note: {remark}");
    }
    if !answer.trim().is_empty() {
        outln!("Answers with {}.", answer.trim());
    }
    let call = if params.iter().all(|p| p.optional) { c.id.to_string() } else { format!("'{}={{…}}'", c.id) };
    outln!("\nRun it with: deckcraft-cli run --cmd {call}");
    Ok(())
}

fn print_params(params: &[deckcraft_engine::cmd::Param]) {
    let name = |p: &deckcraft_engine::cmd::Param| {
        let or = if p.or { "or " } else { "" };
        let optional = if p.optional { ", optional" } else { "" };
        format!("{or}{}{optional}", p.name)
    };
    let width = params.iter().map(|p| name(p).chars().count()).max().unwrap_or_default();
    let sets = params.iter().map(|p| p.object).max().unwrap_or_default() > 0;
    outln!("Parameters:");
    for (k, p) in params.iter().enumerate() {
        if sets && params.get(k.wrapping_sub(1)).is_none_or(|before| before.object != p.object) {
            outln!("  {}", if p.object == 0 { "either:" } else { "or:" });
        }
        let indent = if sets { "    " } else { "  " };
        outln!("{}", format!("{indent}{:<width$}  {}", name(p), p.about).trim_end());
    }
}

fn remark(args: &str) -> &str {
    let after = args.rfind('}').and_then(|i| args.get(i + 1..)).unwrap_or_default().trim().trim_start_matches('—').trim();
    after.strip_prefix('(').and_then(|n| n.strip_suffix(')')).unwrap_or(after)
}

fn app(args: &[String]) -> Result<(), String> {
    use deckcraft_mcp::{Backend, Remote, control_addr};
    let mut port = deckcraft_mcp::DEFAULT_PORT.to_string();
    let mut method: Option<String> = None;
    let mut rest = vec![];
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--port" => port = it.next().cloned().ok_or("--port needs a value")?,
            "--method" => method = Some(it.next().cloned().ok_or("--method needs a name")?),
            other => rest.push(other.to_string()),
        }
    }
    let mut r = Remote::connect(&control_addr(&port))
        .map_err(|e| format!("no DeckCraft app on port {port} ({e}); start it with `deckcraft --control {port}`"))?;
    let parse = |s: Option<&String>| -> Result<Value, String> {
        s.map(|j| serde_json::from_str(j).map_err(|e| format!("bad JSON: {e}"))).transpose().map(|v| v.unwrap_or(json!({})))
    };
    let v = match method {
        Some(m) => r.call(&m, parse(rest.first())?)?,
        None => {
            let id = rest.first().ok_or("app needs a COMMAND or --method")?;
            r.call("engine.execute", json!({"command": id, "params": parse(rest.get(1))?}))?
        }
    };
    outln!("{}", serde_json::to_string_pretty(&v).unwrap_or_default());
    Ok(())
}

fn mcp(args: &[String]) -> Result<(), String> {
    use deckcraft_mcp::{Backend, Headless, Remote, Server, control_addr};
    let mut connect: Option<String> = None;
    let mut sample = false;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--connect" => connect = Some(it.next().cloned().ok_or("--connect needs a port or host:port")?),
            "--sample" => sample = true,
            other => return Err(format!("unknown mcp option `{other}`")),
        }
    }
    let backend: Box<dyn Backend> = match connect {
        Some(c) => Box::new(Remote::connect(&control_addr(&c)).map_err(|e| format!("can't reach the app at {c}: {e}"))?),
        None => {
            let mut h = Headless::new();
            if sample {
                deckcraft_engine::sample::open_sample(&mut h.session).map_err(|e| e.to_string())?;
            }
            Box::new(h)
        }
    };
    let mut server = Server::new(backend);
    let stdin = std::io::stdin();
    server.serve(stdin.lock(), std::io::stdout()).map_err(|e| e.to_string())
}
