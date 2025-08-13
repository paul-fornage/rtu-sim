mod mb_stuff;
mod test_cases;

use log::{info, warn, error, debug};
use std::{
    collections::HashMap,
    future,
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::Duration,
};
use std::fmt::{Debug, Formatter};
use tokio::net::TcpListener;
use dialoguer::{theme::ColorfulTheme, Input, Select};
use local_ip_address::local_ip;
use tokio::time::Instant;
use tokio_modbus::{
    prelude::*,
    server::tcp::{accept_tcp_connection, Server},
};
use tokio_modbus::client::Context;
use crate::mb_stuff::ExampleService;
use crate::test_cases::{sr_single, sr_single_early_stop, EarlyStopResult};

pub const ENABLE_COIL_OFFSET: u16 = 8;
pub const RUNNING_COIL_OFFSET: u16 = 9;
pub const INDEX_HREG_OFFSET: u16 = 8;

// #[derive(Clone)]
// struct Subroutine {
//     index: u16,
//     start_time: Instant,
// }
// impl std::fmt::Debug for Subroutine {
//     fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
//         let elapsed = self.start_time.elapsed().as_millis();
//         write!(f, "Subroutine {{ index: {}, time elapsed: {}ms }}", self.index, elapsed)
//     }
// }

#[derive(Clone, Debug)]
enum State {
    StartSr(u16),
    WaitForRunning(Instant),
    WaitForFinish(Instant),
    DeassertEnable,
    EarlyStopCooldown(Instant),
    DelayBetweenTests
}


#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let socket_addr = "192.168.1.21:502".parse().unwrap();
    env_logger::builder().filter_level(log::LevelFilter::Info).init();
    tokio::select! {
        _ = server_context(socket_addr) => unreachable!(),
        _ = client_context(socket_addr) => info!("Exiting"),
    }

    Ok(())
}
async fn server_context(socket_addr: SocketAddr) -> anyhow::Result<()> {
    info!("Starting up internal server on {socket_addr}");
    let listener = TcpListener::bind(socket_addr).await?;
    let server = Server::new(listener);
    let new_service = |_socket_addr| Ok(Some(ExampleService::new()));
    let on_connected = |stream, socket_addr| async move {
        info!("New connection from {socket_addr}");
        accept_tcp_connection(stream, socket_addr, new_service)
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



async fn client_context(socket_addr: SocketAddr) {

    let color_theme = ColorfulTheme::default();


    // Give the server some time for starting up
    tokio::time::sleep(Duration::from_secs(1)).await;

    info!("Starting!");
    let mut ctx = tcp::connect(socket_addr).await.unwrap();
    info!("Connected to internal server");

    let my_local_ip = local_ip().unwrap();

    info!("Local server IP: {}:502", my_local_ip);

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
            match sr_single(&mut ctx, *index).await {
                Ok(_) => info!("Subroutine {index} completed successfully"),
                Err(err) => {
                    error!("Subroutine failed: {err}");
                    ctx.write_single_coil(ENABLE_COIL_OFFSET, false).await.unwrap().unwrap();
                }
            };
        },
        TestCases::SrUpTo(index) => {
            info!("Arm should execute all sub routines from 0 up to {index} and then stop.");
            for i in 0..*index {
                match sr_single(&mut ctx, i).await{
                    Ok(_) => {
                        debug!("Subroutine {i}/{index} completed.");
                    },
                    Err(err) => {
                        error!("Subroutine failed: {err}");
                        ctx.write_single_coil(ENABLE_COIL_OFFSET, false).await.unwrap().unwrap();
                        break;
                    }
                }
            }
        },
        TestCases::SrOutOfBounds => {
            info!("Arm should execute sub routine 65535 (assumed this does not exist). \
                Just make sure nothing breaks. Could just run a default sr or do nothing \
                as long as running is blipped for enough time to be read true");
            match sr_single(&mut ctx, 65535).await {
                Ok(_) => info!("Subroutine 65535 completed successfully"),
                Err(err) => {
                    error!("Subroutine 65535 failed: {err}")
                }
            }
        },
        TestCases::SrEarlyStopWithDelay(idx, delay) => {
            match sr_single_early_stop(&mut ctx, *idx, Duration::from_millis(*delay as u64)).await {
                Ok(EarlyStopResult::Success) => info!("Subroutine {idx} was stopped early successfully"),
                Ok(EarlyStopResult::TooLate) => warn!("Subroutine {idx} completed before it could be stopped early"),
                Err(err) => {
                    error!("Subroutine {idx} failed stopping early: {err}");
                    ctx.write_single_coil(ENABLE_COIL_OFFSET, false).await.unwrap().unwrap();
                }
            }
        },
        TestCases::SrEarlyStopWithDelayOnAllUpTo(idx, delay) => {
            for i in 0..*idx {
                match sr_single_early_stop(&mut ctx, i, Duration::from_millis(*delay as u64)).await {
                    Ok(EarlyStopResult::Success) => info!("Subroutine {i} was stopped early successfully"),
                    Ok(EarlyStopResult::TooLate) => warn!("Subroutine {i} completed before it could be stopped early"),
                    Err(err) => {
                        error!("Subroutine {i} failed stopping early: {err}");
                        ctx.write_single_coil(ENABLE_COIL_OFFSET, false).await.unwrap().unwrap();
                        break;
                    }
                }
            }
        },
        TestCases::SrEarlyStopAllDelays(idx) => {
            let mut delay = Duration::from_millis(0);
            let mut increment = Duration::from_micros(1);
            let one_second = Duration::from_secs(1);
            loop {
                delay+=increment;
                if increment < one_second {
                    increment*=4;
                }
                debug!("Testing with delay: {:?}", delay);
                match sr_single_early_stop(&mut ctx, *idx, delay).await {
                    Ok(EarlyStopResult::Success) => info!("Subroutine {idx} was stopped early successfully"),
                    Ok(EarlyStopResult::TooLate) => {
                        warn!("Subroutine {idx} completed before it could be stopped early");
                        break;
                    },
                    Err(err) => {
                        error!("Subroutine {idx} failed stopping early: {err}");
                        ctx.write_single_coil(ENABLE_COIL_OFFSET, false).await.unwrap().unwrap();
                        break;
                    }
                }
            }

        }
    }
    info!("Finished test: {:?}", &test_case);

}


