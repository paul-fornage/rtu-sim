use crate::mb_helper::RUNNING_DISCRETE_OFFSET;
use log::{debug, error, info};
use tokio::time::{self, Duration, error};
use tokio_modbus::client::Context;
use crate::mb_helper::{read_running_input, write_en_coil, write_index_hreg};

pub async fn sr_single_shared(ctx: &mut Context, idx: u16) -> anyhow::Result<()> {
    write_index_hreg(ctx, idx).await?;
    write_en_coil(ctx, true).await?;

    let timeout_dur = Duration::from_secs(1);
    let err_msg = format!("Timeout waiting for arm to set `running` to true running \
        subroutine #{idx} at modbus address {RUNNING_DISCRETE_OFFSET} (discrete input). \
        Waited {} ms", timeout_dur.as_millis());
    
    match wait_for_running_shared(ctx, true, timeout_dur).await{
        Ok(WaitForRunningResult::Success) => {},
        Ok(WaitForRunningResult::Timeout) => { return Err(anyhow::anyhow!(err_msg)); },
        Err(e) => { return Err(e); }
    }

    debug!("Arm set to running, should be executing sub routine #{}. Waiting up to 60 seconds for motion to complete", idx);

    let timeout_dur = Duration::from_secs(60);
    
    let err_msg = format!("Timeout waiting for arm to set `running` to false running \
        subroutine #{idx} at modbus address {RUNNING_DISCRETE_OFFSET} (discrete input). \
        Waited {} ms", timeout_dur.as_millis());
    
    match wait_for_running_shared(ctx, false, timeout_dur).await{
        Ok(WaitForRunningResult::Success) => {},
        Ok(WaitForRunningResult::Timeout) => { return Err(anyhow::anyhow!(err_msg)); },
        Err(e) => { return Err(e); }
    }

    debug!("Motion complete");
    write_en_coil(ctx, false).await?;
    time::sleep(Duration::from_millis(100)).await;
    if read_running_input(ctx).await? {
        return Err(anyhow::anyhow!("Arm still running after motion complete. \
            Enable coil was set to false, and then running was set true again. Likely arm is \
            blindly running when enable is true, not only on rising edge"));
    }
    Ok(())
}

pub enum EarlyStopResult {
    Success,
    TooLate,
}


pub async fn sr_single_early_stop_shared(ctx: &mut Context, idx: u16, duration: Duration) -> anyhow::Result<EarlyStopResult> {
    match time::timeout(duration, sr_single_shared(ctx, idx)).await {
        Ok(Ok(())) => {
            debug!("Subroutine #{} completed before the early stop could be initiated", idx);
            Ok(EarlyStopResult::TooLate)
        }
        Ok(Err(e)) => {
            write_en_coil(ctx, false).await?;
            debug!("Subroutine #{} failed to complete before the early stop could be initiated: {}", idx, e);
            Err(e)
        }
        Err(_) => {
            write_en_coil(ctx, false).await?;
            time::sleep(Duration::from_millis(1000)).await;
            if read_running_input(ctx).await? {
                let err_msg = format!("Arm still running after early stop on index: {idx}. \
                    Stopped at {:?} ms and waited 1 second", duration);
                debug!("{}", err_msg);
                return Err(anyhow::anyhow!(err_msg));
            }
            Ok(EarlyStopResult::Success)
        }
    }
}

enum WaitForRunningResult {
    Success,
    Timeout,
}
pub async fn wait_for_running_shared(
    ctx: &mut Context,
    target_state: bool,
    timeout: Duration
) -> anyhow::Result<WaitForRunningResult> {
    match time::timeout(timeout, async {
        loop {
            match read_running_input(ctx).await {
                Ok(actual_state) => {
                    if actual_state == target_state {
                        return Ok(WaitForRunningResult::Success)
                    } else {
                        time::sleep(Duration::from_millis(1)).await;
                    }
                    
                },
                Err(e) => return Err(e),
            }
        }
    }).await {
        Ok(r) => {
            r
        },
        Err(_) => {
            Ok(WaitForRunningResult::Timeout)
        }
    }
}