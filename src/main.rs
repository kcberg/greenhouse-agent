use std::net::Ipv4Addr;

use warp::Filter;

#[tokio::main]
async fn main() {
    println!("Hello, world!");

    let hi = warp::path("hello")
        .and(warp::path::param())
        .and(warp::header("user-agent"))
        .map(|param: String, agent: String| {
            let msg = format!("Hello {}, whose agent is {}", param, agent);
            println!("{}", msg);
            msg
        });


    let addr: Ipv4Addr = Ipv4Addr::new(0, 0, 0, 0);
    warp::serve(hi).bind((addr, 6666 as u16)).await;
}
