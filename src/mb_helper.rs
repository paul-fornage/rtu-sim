use tokio_modbus::client::{Context, Reader, Writer};

// routine_index – 40001
//      This is $SRR[1] in the code above
// enable – 00004
// running – 10005
// Robot Program Select – 00005
//      In order to get the desired robot behavior, this will need to turn true when “routine_index”
//      sets the desired index and turn false when “enable” is set to true.
pub const ENABLE_COIL_OFFSET: u16 = 3;
pub const PROGRAM_SELECT_COIL_OFFSET: u16 = 4;
pub const RUNNING_DISCRETE_OFFSET: u16 = 4;
pub const INDEX_HREG_OFFSET: u16 = 0;

pub async fn write_en_coil(ctx: &mut Context, state: bool) -> anyhow::Result<()> {
    Ok(ctx.write_single_coil(ENABLE_COIL_OFFSET, state).await??)
}
pub async fn write_program_select_coil(ctx: &mut Context, state: bool) -> anyhow::Result<()> {
    Ok(ctx.write_single_coil(PROGRAM_SELECT_COIL_OFFSET, state).await??)
}
pub async fn read_running_input(ctx: &mut Context) -> anyhow::Result<bool> {
    Ok(ctx.read_discrete_inputs(RUNNING_DISCRETE_OFFSET, 1).await??[0])
}
pub async fn write_index_hreg(ctx: &mut Context, index: u16) -> anyhow::Result<()> {
    Ok(ctx.write_single_register(INDEX_HREG_OFFSET, index).await??)
}