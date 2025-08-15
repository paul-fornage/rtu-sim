mod test_cases;
mod mb_helper;

use log::{info, warn, error, debug};
use std::{
    net::SocketAddr,
    time::Duration,
};
use std::fmt::{Debug, Formatter};
use std::net::{Ipv4Addr, SocketAddrV4};

use dialoguer::{theme::ColorfulTheme, Confirm, Input, Select};

use tokio::time::Instant;
use tokio_modbus::prelude::*;
use crate::mb_helper::write_en_coil;
use crate::test_cases::{EarlyStopResult, sr_single, sr_single_early_stop};




const DEFAULT_PORT: u16 = 502; // Default Modbus TCP port


#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {

    env_logger::builder()
        .filter_level(log::LevelFilter::Trace)
        .filter(Some("tokio_modbus"), log::LevelFilter::Info)
        .init();

    let color_theme = ColorfulTheme::default();

    let arm_ip: Ipv4Addr = Input::with_theme(&color_theme)
        .with_prompt("Arm IP address: ")
        .interact_text()
        .unwrap();
    let port: u16 = if Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt("Use default port? (502)")
        .default(true)
        .interact()
        .unwrap()
    {
        DEFAULT_PORT
    } else {
        Input::with_theme(&color_theme)
            .with_prompt("Arm modbus port: ")
            .interact_text()
            .unwrap()
    };
    
    let sock_addr = SocketAddr::V4(SocketAddrV4::new(arm_ip, port));
    
    info!("Connecting to {sock_addr}...");
    let mut ctx = match tcp::connect(sock_addr).await {
        Ok(ctx) => {
            info!("Connected to {sock_addr}!");
            ctx
        }
        Err(err) => {
            error!("Failed to connect to {sock_addr}: {err}");
            return Ok(());
        }
    };
    
    

    // Give the server some time for starting up
    tokio::time::sleep(Duration::from_millis(100)).await;
    
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
                    .with_prompt("How should early stop be tested?")
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
                        test_success = false;
                        write_en_coil(&mut ctx, false).await?;
                    }
                };
            },
            TestCases::SrUpTo(index) => {
                info!("Arm should fully execute all sub routines from 0 up to {index} and then stop.");
                for i in 0..=*index {
                    match sr_single(&mut ctx, i).await {
                        Ok(_) => {
                            info!("Subroutine {i}/{index} completed successfully.");
                        },
                        Err(err) => {
                            error!("Subroutine failed: {err}");
                            test_success = false;
                            write_en_coil(&mut ctx, false).await?;
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
                        write_en_coil(&mut ctx, false).await?;
                        test_success = false;
                        error!("Subroutine 65535 failed: {err}")
                    }
                }
            },
            TestCases::SrEarlyStopWithDelay(idx, delay) => {
                info!("Arm should start execution of sub routine {idx} and then stop after {delay} ms.");
                match sr_single_early_stop(&mut ctx, *idx, Duration::from_millis(*delay as u64)).await {
                    Ok(EarlyStopResult::Success) => info!("Subroutine {idx} was stopped early successfully"),
                    Ok(EarlyStopResult::TooLate) => warn!("Subroutine {idx} completed before it could be stopped early"),
                    Err(err) => {
                        test_success = false;
                        error!("Subroutine {idx} failed stopping early: {err}");
                        write_en_coil(&mut ctx, false).await?;
                    }
                }
            },
            TestCases::SrEarlyStopWithDelayOnAllUpTo(idx, delay) => {
                info!("Arm should start execution of each sub routine [0..={idx}] and stop each one after {delay} ms.");
                for i in 0..=*idx {
                    match sr_single_early_stop(&mut ctx, i, Duration::from_millis(*delay as u64)).await {
                        Ok(EarlyStopResult::Success) => info!("Subroutine {i} was stopped early successfully"),
                        Ok(EarlyStopResult::TooLate) => warn!("Subroutine {i} completed before it could be stopped early"),
                        Err(err) => {
                            test_success = false;
                            error!("Subroutine {i} failed stopping early: {err}");
                            write_en_coil(&mut ctx, false).await?;
                            break;
                        }
                    }
                }
            },
            TestCases::SrEarlyStopAllDelays(idx) => {
                info!("Arm should be given longer and longer periods of time to complete sub routine {idx} until it fully completes");
                let mut delay = Duration::from_millis(5);
                let mut increment = Duration::from_micros(1);
                let max_inc = Duration::from_secs(2);
                loop {
                    delay += increment;
                    if increment < max_inc {
                        increment *= 4;
                    }
                    debug!("Testing with delay: {:?}", delay);
                    match sr_single_early_stop(&mut ctx, *idx, delay).await {
                        Ok(EarlyStopResult::Success) => info!("Subroutine {idx} was stopped early at {:?} successfully", delay),
                        Ok(EarlyStopResult::TooLate) => {
                            warn!("Subroutine {idx} completed before it could be stopped early at {:?}", delay);
                            break;
                        },
                        Err(err) => {
                            test_success = false;
                            error!("Subroutine {idx} failed stopping early at {:?}: {err}", delay);
                            write_en_coil(&mut ctx, false).await?;
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
        { return Ok(()) }
    }
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
