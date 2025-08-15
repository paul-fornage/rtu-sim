use crate::mb_helper::RUNNING_DISCRETE_OFFSET;
use log::{debug, error, info, trace};
use tokio::time::{self, Duration, error};
use tokio_modbus::client::Context;
use crate::mb_helper::{read_running_input, write_en_coil, write_index_hreg};

pub async fn sr_single(ctx: &mut Context, idx: u16) -> anyhow::Result<()> {
    write_index_hreg(ctx, idx).await?;
    write_en_coil(ctx, true).await?;

    let timeout_dur = Duration::from_secs(1);
    let err_msg = format!("Timeout waiting for arm to set `running` to true running \
        subroutine #{idx} at modbus address {RUNNING_DISCRETE_OFFSET} (discrete input). \
        Waited {} ms", timeout_dur.as_millis());
    
    match wait_for_running(ctx, true, timeout_dur).await {
        Ok(WaitForRunningResult::Success) => {},
        Ok(WaitForRunningResult::Timeout) => { return Err(anyhow::anyhow!(err_msg)); },
        Err(e) => { return Err(e); }
    }

    debug!("Arm set to running, should be executing sub routine #{}. Waiting up to 60 seconds for motion to complete", idx);

    let timeout_dur = Duration::from_secs(60);
    
    let err_msg = format!("Timeout waiting for arm to set `running` to false running \
        subroutine #{idx} at modbus address {RUNNING_DISCRETE_OFFSET} (discrete input). \
        Waited {} ms", timeout_dur.as_millis());
    
    match wait_for_running(ctx, false, timeout_dur).await{
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

pub async fn sr_single_early_stop(ctx: &mut Context, idx: u16, early_stop_duration: Duration) -> anyhow::Result<EarlyStopResult> {
    write_index_hreg(ctx, idx).await?;
    write_en_coil(ctx, true).await?;
    
    let end_time = time::Instant::now() + early_stop_duration;

    let running_timeout_dur = Duration::from_secs(1);
    let err_msg = format!("Timeout waiting for arm to set `running` to true running \
        subroutine #{idx} at modbus address {RUNNING_DISCRETE_OFFSET} (discrete input). \
        Waited {} ms", running_timeout_dur.as_millis());
    
    if time::Instant::now() + running_timeout_dur < end_time {
        debug!("Early stop time is after running assert timeout, so we can just wait for running assert");
        match wait_for_running(ctx, true, running_timeout_dur).await {
            Ok(WaitForRunningResult::Success) => {
                debug!("Running asserted before timeout");
            },
            Ok(WaitForRunningResult::Timeout) => { 
                debug!("Running not asserted before timeout");
                return Err(anyhow::anyhow!(err_msg)); 
            },
            Err(e) => { return Err(e); }
        }
    } else {
        let timeout = end_time - time::Instant::now();
        debug!("Early stop time is before running assert timeout, so we can just wait for the early stop");
        match wait_for_running(ctx, true, timeout).await {
            Ok(WaitForRunningResult::Success) => { 
                debug!("Running asserted before early stop");
            },
            Ok(WaitForRunningResult::Timeout) => { 
                // It's time to early stop
                debug!("Running not asserted before early stop, commencing early stop");
                return match execute_early_stop(ctx).await {
                    Ok(_) => Ok(EarlyStopResult::Success),
                    Err(e) => Err(e)
                }
            },
            Err(e) => { return Err(e); }
        }
    }
    

    

    debug!("Arm set to running, should be executing sub routine #{}. Waiting up to 60 seconds for motion to complete", idx);

    let not_running_timeout_dur = Duration::from_secs(60);

    let err_msg = format!("Timeout waiting for arm to set `running` to false running \
        subroutine #{idx} at modbus address {RUNNING_DISCRETE_OFFSET} (discrete input). \
        Waited {} ms", not_running_timeout_dur.as_millis());
    
    if time::Instant::now() + not_running_timeout_dur < end_time {
        debug!("early stop time is after timeout, so we can just wait for the timeout");
        match wait_for_running(ctx, false, not_running_timeout_dur).await{
            Ok(WaitForRunningResult::Success) => {
                debug!("Running deasserted before timeout");
            },
            Ok(WaitForRunningResult::Timeout) => { 
                debug!("Running not deasserted before timeout");
                return Err(anyhow::anyhow!(err_msg)); 
            },
            Err(e) => { return Err(e); }
        }
    } else {
        debug!("early stop time is before deassertion timeout, so we can just wait for the early stop");
        let timeout = end_time - time::Instant::now();
        match wait_for_running(ctx, false, timeout).await {
            Ok(WaitForRunningResult::Success) => {
                debug!("Running deasserted before early stop");
            },
            Ok(WaitForRunningResult::Timeout) => {
                debug!("Running not deasserted before early stop, commencing early stop");
                return match execute_early_stop(ctx).await {
                    Ok(_) => Ok(EarlyStopResult::Success),
                    Err(e) => Err(e)
                }
            },
            Err(e) => { return Err(e); }
        }
    }
    

    debug!("Motion complete");
    write_en_coil(ctx, false).await?;
    time::sleep(Duration::from_millis(100)).await;
    if read_running_input(ctx).await? {
        return Err(anyhow::anyhow!("Arm still running after motion complete. \
            Enable coil was set to false, and then running was set true again. Likely arm is \
            blindly running when enable is true, not only on rising edge"));
    }
    Ok(EarlyStopResult::TooLate)
}

/**
 * To be called mid-operation to stop the arm early.
 */
async fn execute_early_stop(ctx: &mut Context) -> anyhow::Result<()> {
    write_en_coil(ctx, false).await?;
    match wait_for_running(ctx, false, Duration::from_secs(1)).await{
        Ok(WaitForRunningResult::Success) => {
            debug!("Arm early stopped success");
            Ok(())
        },
        Ok(WaitForRunningResult::Timeout) => {
            let err_msg = "Timeout waiting for arm to set `running` to false during early stop. \
                Enable was set to false, but arm still running after 1 second grace period";
            debug!("From `execute_early_stop`: {}", err_msg);
            Err(anyhow::anyhow!(err_msg))
        }
        Err(e) => {
            Err(e)
        }
    }
} 


pub enum WaitForRunningResult {
    Success,
    Timeout,
}
pub async fn wait_for_running(
    ctx: &mut Context,
    target_state: bool,
    timeout: Duration
) -> anyhow::Result<WaitForRunningResult> {
    let start_time = time::Instant::now();
    loop {
        if start_time.elapsed() > timeout {
            trace!("timeout waiting for running");
            return Ok(WaitForRunningResult::Timeout)
        }
        match read_running_input(ctx).await {
            Ok(actual_state) => {
                if actual_state == target_state {
                    return Ok(WaitForRunningResult::Success)
                } else {
                    time::sleep(Duration::from_millis(10)).await;
                }

            },
            Err(e) => return Err(e),
        }
    }
}