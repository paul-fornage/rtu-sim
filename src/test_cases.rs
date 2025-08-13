use log::{debug, error, info};
use tokio::time::{self, Duration, error};
use crate::{ENABLE_COIL_OFFSET, INDEX_HREG_OFFSET, RUNNING_COIL_OFFSET};
use crate::mb_stuff::SharedModbusState;


pub async fn sr_single_shared(shared_state: &SharedModbusState, idx: u16) -> anyhow::Result<()> {
    shared_state.write_holding_register(INDEX_HREG_OFFSET, idx);
    shared_state.write_coil(ENABLE_COIL_OFFSET, true);

    let timeout_dur = Duration::from_secs(1);
    let err_msg = format!("Timeout waiting for arm to set `running` to true running \
        subroutine #{idx} at modbus address {RUNNING_COIL_OFFSET}. \
        Waited {} ms", timeout_dur.as_millis());
    wait_for_running_shared(shared_state, true, timeout_dur).await.map_err(|_| anyhow::anyhow!(err_msg))?;

    debug!("Arm set to running, should be executing sub routine #{}. Waiting up to 60 seconds for motion to complete", idx);

    let timeout_dur = Duration::from_secs(60);
    let err_msg = format!("Timeout waiting for arm to set `running` to false running \
        subroutine #{idx} at modbus address {RUNNING_COIL_OFFSET}. \
        Waited {} ms", timeout_dur.as_millis());
    wait_for_running_shared(shared_state, false, timeout_dur).await.map_err(|_| anyhow::anyhow!(err_msg))?;

    debug!("Motion complete");
    shared_state.write_coil(ENABLE_COIL_OFFSET, false);
    time::sleep(Duration::from_millis(100)).await;
    if shared_state.read_coil(RUNNING_COIL_OFFSET) {
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


pub async fn sr_single_early_stop_shared(shared_state: &SharedModbusState, idx: u16, duration: Duration) -> anyhow::Result<EarlyStopResult> {
    match time::timeout(duration, sr_single_shared(shared_state, idx)).await {
        Ok(Ok(())) => {
            debug!("Subroutine #{} completed before the early stop could be initiated", idx);
            Ok(EarlyStopResult::TooLate)
        }
        Ok(Err(e)) => {
            debug!("Subroutine #{} failed to complete before the early stop could be initiated: {}", idx, e);
            Err(e)
        }
        Err(_) => {
            shared_state.write_coil(ENABLE_COIL_OFFSET, false);
            time::sleep(Duration::from_millis(1000)).await;
            if shared_state.read_coil(RUNNING_COIL_OFFSET) {
                let err_msg = format!("Arm still running after early stop on index: {idx}. \
                    Stopped at {:?} ms and waited 1 second", duration);
                debug!("{}", err_msg);
                return Err(anyhow::anyhow!(err_msg));
            }
            Ok(EarlyStopResult::Success)
        }
    }
}

pub async fn wait_for_running_shared(
    shared_state: &SharedModbusState,
    target_state: bool,
    timeout: Duration
) -> Result<(), error::Elapsed> {
    time::timeout(timeout, async {
        loop {
            if shared_state.read_coil(RUNNING_COIL_OFFSET) == target_state {
                return(());
            }
            time::sleep(Duration::from_millis(1)).await;
        }
    }).await
}