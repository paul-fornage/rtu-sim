use std::collections::HashMap;
use std::future;
use std::sync::{Arc, Mutex};
use tokio_modbus::{ExceptionCode, Request, Response};
use crate::{ENABLE_COIL_OFFSET, INDEX_HREG_OFFSET, RUNNING_COIL_OFFSET};

pub struct ExampleService {
    holding_registers: Arc<Mutex<HashMap<u16, u16>>>,
    coils: Arc<Mutex<HashMap<u16, bool>>>,
}

impl tokio_modbus::server::Service for ExampleService {
    type Request = Request<'static>;
    type Response = Response;
    type Exception = ExceptionCode;
    type Future = future::Ready<Result<Self::Response, Self::Exception>>;

    fn call(&self, req: Self::Request) -> Self::Future {
        let res = match req {
            Request::ReadHoldingRegisters(addr, cnt) => {
                register_read(&self.holding_registers.lock().unwrap(), addr, cnt)
                    .map(Response::ReadHoldingRegisters)
            }
            Request::WriteMultipleRegisters(addr, values) => {
                register_write(&mut self.holding_registers.lock().unwrap(), addr, &values)
                    .map(|_| Response::WriteMultipleRegisters(addr, values.len() as u16))
            }
            Request::WriteSingleRegister(addr, value) => register_write(
                &mut self.holding_registers.lock().unwrap(),
                addr,
                std::slice::from_ref(&value),
            ).map(|_| Response::WriteSingleRegister(addr, value)),
            Request::ReadCoils(addr, cnt) => {
                coil_read(&mut self.coils.lock().unwrap(), addr, cnt).map(Response::ReadCoils)
            }
            Request::WriteMultipleCoils(addr, values) => {
                coil_write(&mut self.coils.lock().unwrap(), addr, &values).map(|_| Response::WriteMultipleCoils(addr, values.len() as u16))
            }
            Request::WriteSingleCoil(addr, value) => {
                coil_write(&mut self.coils.lock().unwrap(), addr, std::slice::from_ref(&value)).map(|_| Response::WriteSingleCoil(addr, value))
            }
            _ => {
                println!("SERVER: Exception::IllegalFunction - Unimplemented function code in request: {req:?}");
                Err(ExceptionCode::IllegalFunction)
            }
        };
        future::ready(res)
    }
}

impl ExampleService {
    pub(crate) fn new() -> Self {
        // Insert some test data as register values.
        let mut coils = HashMap::new();
        coils.insert(ENABLE_COIL_OFFSET, false);
        coils.insert(RUNNING_COIL_OFFSET, false);
        let mut holding_registers = HashMap::new();
        holding_registers.insert(INDEX_HREG_OFFSET, 0);

        Self {
            coils: Arc::new(Mutex::new(coils)),
            holding_registers: Arc::new(Mutex::new(holding_registers)),
        }
    }
}

/// Helper function implementing reading registers from a HashMap.
fn register_read(
    registers: &HashMap<u16, u16>,
    addr: u16,
    cnt: u16,
) -> Result<Vec<u16>, ExceptionCode> {
    let mut response_values = vec![0; cnt.into()];
    for i in 0..cnt {
        let reg_addr = addr + i;
        if let Some(r) = registers.get(&reg_addr) {
            response_values[i as usize] = *r;
        } else {
            println!("SERVER: Exception::IllegalDataAddress");
            return Err(ExceptionCode::IllegalDataAddress);
        }
    }

    Ok(response_values)
}

/// Write a holding register. Used by both the write single register
/// and write multiple registers requests.
fn register_write(
    registers: &mut HashMap<u16, u16>,
    addr: u16,
    values: &[u16],
) -> Result<(), ExceptionCode> {
    for (i, value) in values.iter().enumerate() {
        let reg_addr = addr + i as u16;
        if let Some(r) = registers.get_mut(&reg_addr) {
            *r = *value;
        } else {
            println!("SERVER: Exception::IllegalDataAddress");
            return Err(ExceptionCode::IllegalDataAddress);
        }
    }

    Ok(())
}

fn coil_read(
    coils: &HashMap<u16, bool>,
    addr: u16,
    cnt: u16,
) -> Result<Vec<bool>, ExceptionCode> {
    let mut response_values = vec![false; cnt.into()];
    for i in 0..cnt {
        let reg_addr = addr + i;
        if let Some(r) = coils.get(&reg_addr) {
            response_values[i as usize] = *r;
        }
    }
    Ok(response_values)
}

fn coil_write(
    coils: &mut HashMap<u16, bool>,
    addr: u16,
    values: &[bool],
) -> Result<(), ExceptionCode> {
    for (i, value) in values.iter().enumerate() {
        let reg_addr = addr + i as u16;
        if let Some(r) = coils.get_mut(&reg_addr) {
            *r = *value;
        }
    }
    
    Ok(())
}
