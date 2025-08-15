mod mb_stuff;
mod test_cases;

use log::{info, warn, error, debug};
use std::{
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::Duration,
};
use std::fmt::{Debug, Formatter};
use std::net::{IpAddr, Ipv4Addr, SocketAddrV4};
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::net::TcpListener;
use dialoguer::{theme::ColorfulTheme, Confirm, Input, Select};
use local_ip_address::local_ip;
use tokio::time::Instant;
use tokio_modbus::{
    prelude::*,
    server::tcp::{accept_tcp_connection, Server},
};
use crate::mb_stuff::{ExampleService, SharedModbusState};
use crate::test_cases::{EarlyStopResult, sr_single_shared, sr_single_early_stop_shared};

pub const ENABLE_COIL_OFFSET: u16 = 8;
pub const RUNNING_COIL_OFFSET: u16 = 9;
pub const INDEX_HREG_OFFSET: u16 = 8;
static CLIENT_CONNECTED: AtomicBool = AtomicBool::new(false);
const DEFAULT_PORT: u16 = 502; // Default Modbus TCP port


#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {

    let args: Vec<String> = std::env::args().collect();
    let port = parse_port_arg(&args)?;
    
    let ip = local_ip().unwrap();
    let ipv4 = match ip{
        IpAddr::V4(v4) => v4,
        IpAddr::V6(_) => panic!("Local IP says IPv6. This is not supported and highly unlikely for a local ip")
    };
    let sock_addr: SocketAddr = SocketAddr::V4(SocketAddrV4::new(ipv4, port));
    env_logger::builder().filter_level(log::LevelFilter::Info).init();
    
    // Create shared state
    let shared_state = SharedModbusState::new();
    let shared_state_clone = shared_state.clone();

    let server_handle = tokio::spawn(server_context(sock_addr, shared_state));

    // Run client (with blocking TUI) in a separate thread
    let client_handle = std::thread::spawn(move || {
        // Use a runtime in this thread for the async parts
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(tui_thread(shared_state_clone))
    });

    // Wait for client to finish
    client_handle.join().unwrap();
    // Optionally abort the server when client is done
    server_handle.abort();

    Ok(())
}


fn parse_port_arg(args: &[String]) -> Result<u16, Box<dyn std::error::Error>> {
    for i in 0..args.len() {
        if args[i] == "--port" || args[i] == "-p" {
            if i + 1 >= args.len() {
                return Err("Port argument requires a value".into());
            }
            let port_str = &args[i + 1];
            let port: u16 = port_str.parse()
                .map_err(|_| format!("Invalid port number: {}", port_str))?;
            if port == 0 {
                return Err("Port number must be greater than 0".into());
            }
            return Ok(port);
        }
    }
    Ok(DEFAULT_PORT)
}


async fn server_context(socket_addr: SocketAddr, shared_state: SharedModbusState) -> anyhow::Result<()> {
    info!("Starting up local server on {socket_addr}");
    let listener = TcpListener::bind(socket_addr).await?;
    let server = Server::new(listener);

    let on_connected = move |stream, socket_addr| {
        let shared_state = shared_state.clone();
        CLIENT_CONNECTED.store(true, Ordering::Relaxed);
        let new_service = move |_socket_addr| {
            let state = shared_state.clone();
            Ok(Some(ExampleService::with_shared_state(state)))
        };
        async move {
            info!("New connection from {socket_addr}");
            accept_tcp_connection(stream, socket_addr, new_service)
        }
    };

    let on_process_error = |err| {
        error!("{err}");
    };
    server.serve(&on_connected, on_process_error).await?;
    Ok(())
}


enum TestCases {
    SrSingle(u16),
    SrUpTo(u16),
    SrOutOfBounds,
    SrEarlyStopWithDelay(u16, u16),
    SrEarlyStopWithDelayOnAllUpTo(u16, u16),
    SrEarlyStopAllDelays(u16),
}

impl Debug for TestCases {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            TestCases::SrSingle(index) =>
                write!(f, "Test single sub routine: {}", index),
            TestCases::SrUpTo(index) =>
                write!(f, "Test all sub routines up to {}", index),
            TestCases::SrOutOfBounds =>
                write!(f, "Test out of bounds sub routine."),
            TestCases::SrEarlyStopWithDelay(index, delay) =>
                write!(f, "Test sub routine #{} early stop with delay: {}", index, delay),
            TestCases::SrEarlyStopAllDelays(index) =>
                write!(f, "Test sub routine #{} early stop with all delays", index),
            TestCases::SrEarlyStopWithDelayOnAllUpTo(index, delay) => {
                write!(f, "Test all sub routines up to #{} early stop with delay {}", index, delay)
            }
        }
    }
}

async fn tui_thread(shared_state: SharedModbusState) {
    let color_theme = ColorfulTheme::default();

    // Give the server some time for starting up
    tokio::time::sleep(Duration::from_secs(1)).await;
    if !CLIENT_CONNECTED.load(Ordering::Relaxed) {
        warn!("No client connected yet. Waiting for connection...");
        while(!CLIENT_CONNECTED.load(Ordering::Relaxed)) {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    } else {
        info!("Client is connected - ready to run tests");
    }
    let mut test_success;
    
    loop {

        

        test_success = true;
        let selections = &[
            "Execute SR",
            "Early stop",
            "Out of bounds",
        ];

        let selection = Select::with_theme(&color_theme)
            .with_prompt("Select a test case")
            .default(0)
            .items(&selections[..])
            .interact()
            .unwrap();

        info!("Running test: {}!", selections[selection]);
        let test_case: TestCases = match selection {
            0 => { // Execute SR
                let selection = Select::with_theme(&color_theme)
                    .with_prompt("What routines to test")
                    .default(0)
                    .items(&["Single manual index", "All indices up to user specified value"])
                    .interact()
                    .unwrap();
                let index: u16 = Input::with_theme(&color_theme)
                    .with_prompt("Sub routine index: ")
                    .interact_text()
                    .unwrap();
                if selection == 0 {
                    TestCases::SrSingle(index)
                } else {
                    TestCases::SrUpTo(index)
                }
            },
            1 => {
                let selection = Select::with_theme(&color_theme)
                    .with_prompt("How should early stop be tested? (How long to wait before early stop)")
                    .default(0)
                    .items(&[
                        "All delays on specific sub routine",
                        "Specific delay on specific sub routine",
                        "Specific delay on all sub routines up to \'n\'"])
                    .interact()
                    .unwrap();
                if selection == 0 {
                    let index: u16 = Input::with_theme(&color_theme)
                        .with_prompt("Sub routine index: ")
                        .interact_text()
                        .unwrap();
                    TestCases::SrEarlyStopAllDelays(index)
                } else if selection == 1 {
                    let index: u16 = Input::with_theme(&color_theme)
                        .with_prompt("Sub routine index: ")
                        .interact_text()
                        .unwrap();
                    let delay: u16 = Input::with_theme(&color_theme)
                        .with_prompt("Delay after writing enable high to cancel op (ms)")
                        .interact_text()
                        .unwrap();
                    TestCases::SrEarlyStopWithDelay(index, delay)
                } else {
                    let index: u16 = Input::with_theme(&color_theme)
                        .with_prompt("Test all sub routines up to index: ")
                        .interact_text()
                        .unwrap();
                    let delay: u16 = Input::with_theme(&color_theme)
                        .with_prompt("Delay after writing enable high to cancel op (ms)")
                        .interact_text()
                        .unwrap();
                    TestCases::SrEarlyStopWithDelayOnAllUpTo(index, delay)
                }
            }
            _ => {
                TestCases::SrOutOfBounds
            }
        };

        info!("Test selected: \n\t{test_case:?}");

        match &test_case {
            TestCases::SrSingle(index) => {
                info!("Arm should execute sub routine: {index} and then stop.");
                match sr_single_shared(&shared_state, *index).await {
                    Ok(_) => info!("Subroutine {index} completed successfully"),
                    Err(err) => {
                        error!("Subroutine failed: {err}");
                        test_success = false;
                        shared_state.write_coil(ENABLE_COIL_OFFSET, false);
                    }
                };
            },
            TestCases::SrUpTo(index) => {
                info!("Arm should fully execute all sub routines from 0 up to {index} and then stop.");
                for i in 0..=*index {
                    match sr_single_shared(&shared_state, i).await {
                        Ok(_) => {
                            info!("Subroutine {i}/{index} completed successfully.");
                        },
                        Err(err) => {
                            error!("Subroutine failed: {err}");
                            test_success = false;
                            shared_state.write_coil(ENABLE_COIL_OFFSET, false);
                            break;
                        }
                    }
                }
            },
            TestCases::SrOutOfBounds => {
                info!("Arm should execute sub routine 65535 (assumed this does not exist). \
                Just make sure nothing breaks. Could just run a default sr or do nothing \
                as long as running is blipped for enough time to be read true");
                match sr_single_shared(&shared_state, 65535).await {
                    Ok(_) => info!("Subroutine 65535 completed successfully"),
                    Err(err) => {
                        test_success = false;
                        error!("Subroutine 65535 failed: {err}")
                    }
                }
            },
            TestCases::SrEarlyStopWithDelay(idx, delay) => {
                info!("Arm should start execution of sub routine {idx} and then stop after {delay} ms.");
                match sr_single_early_stop_shared(&shared_state, *idx, Duration::from_millis(*delay as u64)).await {
                    Ok(EarlyStopResult::Success) => info!("Subroutine {idx} was stopped early successfully"),
                    Ok(EarlyStopResult::TooLate) => warn!("Subroutine {idx} completed before it could be stopped early"),
                    Err(err) => {
                        test_success = false;
                        error!("Subroutine {idx} failed stopping early: {err}");
                        shared_state.write_coil(ENABLE_COIL_OFFSET, false);
                    }
                }
            },
            TestCases::SrEarlyStopWithDelayOnAllUpTo(idx, delay) => {
                info!("Arm should start execution of each sub routine [0..={idx}] and stop each one after {delay} ms.");
                for i in 0..=*idx {
                    match sr_single_early_stop_shared(&shared_state, i, Duration::from_millis(*delay as u64)).await {
                        Ok(EarlyStopResult::Success) => info!("Subroutine {i} was stopped early successfully"),
                        Ok(EarlyStopResult::TooLate) => warn!("Subroutine {i} completed before it could be stopped early"),
                        Err(err) => {
                            test_success = false;
                            error!("Subroutine {i} failed stopping early: {err}");
                            shared_state.write_coil(ENABLE_COIL_OFFSET, false);
                            break;
                        }
                    }
                }
            },
            TestCases::SrEarlyStopAllDelays(idx) => {
                info!("Arm should be given longer and longer periods of time to complete sub routine {idx} until it fully completes");
                let mut delay = Duration::from_millis(0);
                let mut increment = Duration::from_micros(1);
                let max_inc = Duration::from_secs(2);
                loop {
                    delay += increment;
                    if increment < max_inc {
                        increment *= 4;
                    }
                    debug!("Testing with delay: {:?}", delay);
                    match sr_single_early_stop_shared(&shared_state, *idx, delay).await {
                        Ok(EarlyStopResult::Success) => info!("Subroutine {idx} was stopped early at {:?} successfully", delay),
                        Ok(EarlyStopResult::TooLate) => {
                            warn!("Subroutine {idx} completed before it could be stopped early at {:?}", delay);
                            break;
                        },
                        Err(err) => {
                            test_success = false;
                            error!("Subroutine {idx} failed stopping early at {:?}: {err}", delay);
                            shared_state.write_coil(ENABLE_COIL_OFFSET, false);
                            break;
                        }
                    }
                }
            }
        }
        info!("Finished test: {:?}", &test_case);
        if test_success {
            info!("✅ Test was successful!");
        } else {
            error!("❌ Test failed!")
        }

        if !Confirm::with_theme(&color_theme)
            .with_prompt("Do you want to continue?")
            .default(true)
            .interact()
            .unwrap()
        { return }
    }
}