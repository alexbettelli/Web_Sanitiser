use sanitiser_core::config::Policy;
use clap::Parser;
use sanitiser_core::input::{self, Input};

#[derive(Parser, Debug)]
#[command(name = "web-sanitizer", version, about = "Sanitiser per contenuti web")]
struct Args {
    /// File, directory o liste di URL da elaborare (batch mode: più
    /// argomenti sono ammessi).
    #[arg(required = true)]
    inputs: Vec<String>,
}

fn main() -> anyhow::Result<()> {

    let args = Args::parse();
    let resolved = input::resolve_args(&args.inputs)?;
    let _policy = Policy::default();

    for item in &resolved {
        match item {
            Input::File(p) => println!("[file]      {}", p.display()),
            Input::Directory(p) => {
                println!("[directory] {}", p.display());
                for f in input::walk_directory(p)? {
                    println!("            -> {} ({:?})", f.relative_path.display(), f.kind);
                }
            }
            Input::Url(u) => println!("[url]       {}", u),
        }
    }
    Ok(())
}
