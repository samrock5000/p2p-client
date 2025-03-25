use std::{env, net, thread, io};
use std::ops::ControlFlow;
use std::io::Read;
use std::io::BufRead;
mod error;
use std::str::Lines;

use argh::FromArgs;
use client::{Loading, Client, Config};
mod logger;
use nakamoto_cash::client::traits::Handle;
use nakamoto_cash::client::{self, Network};
// use nakamoto_cash::common::bitcoin::util::bloom::BloomFilter;
type Reactor = nakamoto_cash::net::poll::Reactor<net::TcpStream>;
use crossbeam_channel::{self as chan};
mod bloom;
use bloom::{Bloom,BloomFilter};
use std::io::Write;
pub struct Watcher<H> {
    client: H,
    network: Network,
    bloom: BloomFilter,
}

impl<H: Handle> Watcher<H> {
    pub fn new(client: H, network: client::Network) -> Self {
        let filter = Bloom::<u8>::new_for_fp_rate(10_000, 0.01);
        let bloom = BloomFilter::from(filter);
        Self {
            client,
            network,
            bloom,
        }
    }

    /// Run the wallet loop until it exits.
    pub fn run(
        &mut self,
        loading: &chan::Receiver<client::Loading>,
        events: &chan::Receiver<client::Event>,
        ui_rx: &chan::Receiver<Vec<String>>,
    ) -> Result<(), error::Error> {
        loop {
            chan::select! {
                recv(loading) -> event => {
                    if let Ok(event) = event {
                        if let ControlFlow::Break(()) = self.handle_loading_event(event)? {
                            return Ok(());
                        }
                    } else {
                        break;
                    }
                }
                recv(events) -> event => {
                    let event = event?;
                    if let ControlFlow::Break(()) = 
                        self.handle_client_event(event)? { 
                        break;
                    }
                }
                recv(ui_rx) -> event => {
                    let event = event?;
                    if let ControlFlow::Break(()) = 
                        self.handle_user_input(event)? { 
                        break;
                    }
                }
            }
        }
        Ok(())
    }

    fn handle_client_event(
        &mut self,
        event: client::Event,
    ) -> Result<ControlFlow<()>, error::Error> {
        match event {
            client::Event::Ready { tip, .. } => {
                log::info!(target: "watcher ready at tip", "{}", tip);
            }
            _ => {}  // Added catch-all for unhandled events
        }
        Ok(ControlFlow::Continue(()))
    }

    fn handle_loading_event(
        &mut self,
        event: client::Loading,
    ) -> Result<ControlFlow<()>, error::Error> {
        match event {
            Loading::BlockHeaderLoaded { height } => {
                print!("\r HEADERS LOADING {}\r",height);
            }
            _ => {}
        }
        Ok(ControlFlow::Continue(()))
    }
    
    fn handle_user_input(
        &mut self,
        event:Vec<String> ,
    ) -> Result<ControlFlow<()>, error::Error> {
        for argument in env::args() {
            println!("{argument}");
}
        Ok(ControlFlow::Continue(()))
    }
}

// Rest of your code remains the same...
/// A Bitcoin wallet.
#[derive(FromArgs)]
pub struct Options {
    /// network to connect to, eg. `chipnet`
    #[argh(option, default = "Network::default()")]
    pub network: Network,
    /// connect to this node
    #[argh(option)]
    pub connect: Vec<net::SocketAddr>,
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

    let (loading_tx, loading_rx) = chan::unbounded();
    let (ui_input_tx, ui_input_rx) = chan::unbounded();

    let client_recv = handle.events();

    let cfg = Config {
        network,
        connect,
        listen: vec![],
        ..Config::default()
    };

    logger::init(level).expect("initializing logger for the first time");

    let t1 = thread::spawn(move || client.load(cfg, loading_tx)?.run());
    let t2 = thread::spawn(move || {
        if let Err(err) = Watcher::new(handle.clone(), network).run(
            &loading_rx,
            &client_recv,
            &ui_input_rx,
        ) {
            println!("FATAL ERR {:?}", err);
            std::process::exit(1);
        };
    });

    // New thread for handling user input from terminal
    let t3 = thread::spawn(move || {
        let tx_handle = ui_input_tx.clone();
        let stdin = io::stdin().lock();

    for event in stdin.events() {
        
    }
        let event = event?;
        loop {
    });

    // t1.join().unwrap().unwrap();
    t2.join().unwrap();
    t3.join().unwrap();
}

impl Options {
    pub fn from_env() -> Self {
        argh::from_env()
    }
}
