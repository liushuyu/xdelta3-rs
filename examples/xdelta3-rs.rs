use async_std::fs::File;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
enum Mode {
    Encode,
    Decode,
}

#[derive(Debug, StructOpt)]
struct Opt {
    #[structopt(subcommand)]
    mode: Mode,

    #[structopt(short = "i")]
    input: String,
    #[structopt(short = "s")]
    source: String,
    #[structopt(short = "o")]
    output: String,
}

async fn run(opt: Opt) {
    let input = File::open(&opt.input).await.expect("File::open");
    let source = File::open(&opt.source).await.expect("File::open");
    let out = File::create(&opt.output).await.expect("File::create");

    match opt.mode {
        Mode::Decode => {
            xdelta3::decode_async(input, source, out)
                .await
                .expect("failed to decode");
        }
        Mode::Encode => {
            xdelta3::encode_async(input, source, out)
                .await
                .expect("failed to encode");
        }
    }
}

fn main() {
    env_logger::init();

    let opt = Opt::from_args();

    async_std::task::block_on(run(opt));
}
