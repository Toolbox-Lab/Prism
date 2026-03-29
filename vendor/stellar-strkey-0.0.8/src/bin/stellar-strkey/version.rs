use clap::Parser;

#[derive(Parser, Debug, Clone)]
#[command()]
pub struct Cmd;

impl Cmd {
    pub fn run() {
        let v = stellar_strkey::VERSION;
        println!("stellar-strkey {} ({})", v.pkg, v.rev);
    }
}
