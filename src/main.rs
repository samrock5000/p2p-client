use std::{env, net, thread};
use argh::FromArgs;
use client::{Client, Config};
mod logger;
use nakamoto_cash::client::traits::Handle;
use nakamoto_cash::client::{self, Network};
type Reactor = nakamoto_cash::net::poll::Reactor<net::TcpStream>;
use crossbeam_channel::{self as chan};

/// A Bitcoin wallet.
#[derive(FromArgs)]
pub struct Options {
    /// network to connect to, eg. `chipnet
    #[argh(option, default = "Network::default()")]
    pub network: Network,
    /// connect to this node
    #[argh(option)]
    pub connect: Vec<net::SocketAddr>,
    // / wallet file
    // #[argh(option)]
    // pub wallet: Option<PathBuf>,
    /// enable debug logging
    #[argh(switch)]
    pub debug: Option<bool>,
}


fn main() {

    let opts = Options::from_env();
    let client = Client::<Reactor>::new().unwrap();
    let handle = client.handle();
    let network = opts.network;
    let addrs = opts.connect;
    let connect = addrs;

    let level = if opts.debug.is_some() {
        log::Level::Debug
    } else {
        log::Level::Error
    };


    let (tx, rx) = chan::unbounded();
    
    let cfg = Config {
        network,
        connect,
        // root: PathBuf::from(env::var(HOME_DIR).unwrap_or_default()),
        listen: vec![], // Don't listen for incoming connections.
        ..Config::default()
    };

    logger::init(level).expect("initializing logger for the first time");

  let t1 = thread::spawn(move || client.load(cfg, tx)?.run());


  t1.join().unwrap().unwrap();


    // thread::spawn(move || {
    //     (0..10).for_each(|i| {
    //         tx.send(i).unwrap();
    //     })
    // });

    // let received: u32 = rx.iter().sum();

    // assert_eq!((0..10).sum::<u32>(), received);
}
impl Options {
    pub fn from_env() -> Self {
        argh::from_env()
    }
}

