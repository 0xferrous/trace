use axum::{Router, routing::get};
use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "backend")]
#[command(about = "Backend server", long_about = None)]
struct Args {
    /// Port to listen on
    #[arg(short, long, env = "PORT", default_value = "3000")]
    port: u16,
}

async fn hello() -> &'static str {
    "Hello, World!"
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    let app = Router::new().route("/", get(hello));

    let addr = format!("127.0.0.1:{}", args.port);
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();

    println!("Server running on http://{}", addr);

    axum::serve(listener, app).await.unwrap();
}
