use std::{env, net, thread, io};
use std::ops::ControlFlow;
// use std::io::Read;
// use std::io::BufRead;
mod error;
// use std::str::Lines;
use slint::{Model, ModelRc, SharedString};
use std::{rc::Rc, str::FromStr};

use slint::PlatformError;

slint::include_modules!();

use argh::FromArgs;
use client::{Loading, Client, Config,Event,Peer};
mod logger;
use nakamoto_cash::client::traits::Handle;
use nakamoto_cash::client::{self, Network};
// use nakamoto_cash::common::bitcoin::util::bloom::BloomFilter;
type Reactor = nakamoto_cash::net::poll::Reactor<net::TcpStream>;
use crossbeam_channel::{self as chan, Receiver, Sender};
mod bloom;
use bloom::{Bloom,BloomFilter};
use std::io::Write;

#[derive(Clone,Debug)]
pub enum UIMessage {
    HeaderLoaded(u64),
    PeerHeightUpdate(u64),
    AddBlooomItem(String)
}

pub struct Watcher<H> {
    client: H,
    network: Network,
    peers: Vec<Peer>,
    bloom: BloomFilter,
}

impl<H: Handle> Watcher<H> {
    pub fn new(client: H, network: client::Network) -> Self {
        let filter = Bloom::<u8>::new_for_fp_rate(10_000, 0.01);
        let bloom = BloomFilter::from(filter);
        let peers: Vec<Peer> = Vec::with_capacity(32);
        Self {
            client,
            network,
            peers,
            bloom,
        }
    }

    /// Run the watch loop until it exits.
    pub fn run(
        &mut self,
        loading: &Receiver<client::Loading>,
        events: &Receiver<client::Event>,
        ui_input_rx: &Receiver<UIMessage>,
        ui_show_tx: &Sender<UIMessage>,

    ) -> Result<(), error::Error> {
        loop {
            chan::select! {
                recv(loading) -> event => {
                    if let Ok(event) = event {
                        if let ControlFlow::Break(()) = self.handle_loading_event(event,ui_show_tx)? {
                            return Ok(());
                        }
                    } else {
                        break;
                    }
                }
                recv(events) -> event => {
                    let event = event?;
                    if let ControlFlow::Break(()) = 
                        self.handle_client_event(event,ui_show_tx)? { 
                        break;
                    }
                }
                recv(ui_input_rx) -> event => {
                    let event = event?;
                    if let ControlFlow::Break(()) = 
                        self.update_user_input(event)? { 
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
        ui_show_tx: &chan::Sender<UIMessage>,
    ) -> Result<ControlFlow<()>, error::Error> {
        match event {
            Event::Ready { tip, .. } => {
                // log::info!(target: "watcher ready at tip", "{}", tip);
            }
            Event::PeerNegotiated {height, .. } => {
                _ = ui_show_tx.send(UIMessage::PeerHeightUpdate(height))
                
            }
            _ => {}  // Added catch-all for unhandled events
        }
        Ok(ControlFlow::Continue(()))
    }

    fn handle_loading_event(
        &mut self,
        event: client::Loading,
        ui_show_tx: &chan::Sender<UIMessage>,
    ) -> Result<ControlFlow<()>, error::Error> {
        match event {
            Loading::BlockHeaderLoaded { height } => {
                _ = ui_show_tx.send(UIMessage::HeaderLoaded(height))
            }
            Loading::BlockHeaderLoadComplete => {
                //TODO inform UI?
            }
            _ => {}
        }
        Ok(ControlFlow::Continue(()))
    }
    fn update_user_input(
        &mut self,
        ui_input: UIMessage,
    ) -> Result<ControlFlow<()>, error::Error> {
              println!("BLOOM ITEM RECV {:?}",ui_input) ;            
            match ui_input {
              UIMessage::AddBlooomItem(data) => {
              println!("BLOOM ITEM RECV {:?}",data) ;            
                }
            UIMessage::HeaderLoaded(msg) => {}
                _ => {},
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
    let (ui_show_tx, ui_show_rx) = chan::unbounded();
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
            &ui_show_tx,
        ) {
            println!("FATAL ERR {:?}", err);
            std::process::exit(1);
        };
    });

    // New thread for handling user input from terminal
    _ = run_ui_main(&ui_input_tx ,&ui_show_rx);
    t1.join().unwrap().unwrap();
    t2.join().unwrap();
    // t3.join().unwrap();
}

pub fn run_ui_main(
        ui_input_tx: &Sender<UIMessage>,
        ui_show_rx: &Receiver<UIMessage>,
) -> Result<(), PlatformError> {
        let filter_items = Rc::new(slint::VecModel::<SharedString>::from(vec![]));

        let main_window = MainWindow::new()?;
        let ui_show_rx = ui_show_rx.clone();

        let app = main_window.as_weak().clone();
        let tx_handle = ui_input_tx.clone();
        
        app.unwrap().on_load_bloom_item(move || {
        let items = app.unwrap().get_filter_items();
        let item = app.unwrap().get_bloom_item();
        
        _ = tx_handle.send(UIMessage::AddBlooomItem(item.clone() .into()));
        println!("FILTER ITEM {:?}", item);
        filter_items.push(item);
        items.iter().for_each(|f| {
            filter_items.push(f.clone());
        });

        println!("FILTER ITEMs {:?}", filter_items.row_count());
        // let items = filter_items
        //     .iter()
        //     .map(|i| i.to_string())
        //     .collect::<Vec<_>>();
        // items.iter().for_each(|f| {
        // });
        // let payload = json::to_string(&items);

        // let msg = Payload {
        //     message: "load-item".to_string(),
        //     payload,
        // };
        // _ = tx_handle.send(msg);
    });



        
        let app = main_window.as_weak().clone();
            std::thread::spawn(move || {
            loop {
                chan::select! {
                    recv(ui_show_rx) -> event => {
                        if let Ok(event) = event {
                            _ = app .upgrade_in_event_loop(move |app| {
                             match event {
                                UIMessage::HeaderLoaded(height) => {
                                    app.as_weak().unwrap().set_loaded_header(height.to_string().into());
                                }
                                UIMessage::PeerHeightUpdate(height) => {
                                    app.as_weak().unwrap().set_loaded_header(height.to_string().into());
                                }
                                UIMessage::AddBlooomItem(item) => {
                                    app.as_weak().unwrap().set_bloom_item(item.into()) 
                                }
                            }
                                
                            });

                  
                      }
                   }
                }

            }                
         });


        main_window.run()
}

impl Options {
    pub fn from_env() -> Self {
        argh::from_env()
    }
}
