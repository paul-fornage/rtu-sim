use std::collections::HashMap;
use std::future;
use std::sync::{Arc, Mutex};
use log::warn;
use tokio_modbus::{ExceptionCode, Request, Response};
use crate::{ENABLE_COIL_OFFSET, INDEX_HREG_OFFSET, RUNNING_COIL_OFFSET};

#[derive(Clone)]
pub struct SharedModbusState {
    holding_registers: Arc<Mutex<HashMap<u16, u16>>>,
    coils: Arc<Mutex<HashMap<u16, bool>>>,
}

impl SharedModbusState {
    pub fn new() -> Self {
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

    pub fn read_coil(&self, addr: u16) -> bool {
        let coils = self.coils.lock().unwrap();
        if let Some(&value) = coils.get(&addr) {
            value
        } else {
            warn!("Attempted to read from non-existent coil {addr}");
            false
        }
    }

    pub fn read_coils(&self, addr: u16, count: u16) -> Vec<bool> {
        let coils = self.coils.lock().unwrap();
        let mut result = Vec::with_capacity(count as usize);
        for i in 0..count {
            let coil_addr = addr + i;
            if let Some(&value) = coils.get(&coil_addr) {
                result.push(value);
            } else {
                warn!("Attempted to read from non-existent coil {coil_addr}");
                result.push(false);
            }
        }
        result
    }

    pub fn write_coil(&self, addr: u16, value: bool) {
        if let Some(coil) = self.coils.lock().unwrap().get_mut(&addr) {
            *coil = value;
        } else {
            warn!("Attempted to write to non-existent coil {addr}");
        }
    }

    pub fn write_coils(&self, addr: u16, values: &[bool]) {
        let mut coils = self.coils.lock().unwrap();
        for (i, &value) in values.iter().enumerate() {
            let coil_addr = addr + i as u16;
            if let Some(coil) = coils.get_mut(&coil_addr) {
                *coil = value;
            } else {
                warn!("Attempted to write to non-existent coil {coil_addr}");
            }
        }
    }

    pub fn read_holding_registers(&self, addr: u16, count: u16) -> Vec<u16> {
        let registers = self.holding_registers.lock().unwrap();
        let mut result = Vec::with_capacity(count as usize);
        for i in 0..count {
            let reg_addr = addr + i;
            if let Some(&value) = registers.get(&reg_addr) {
                result.push(value);
            } else {
                warn!("Attempted to read from non-existent holding register {reg_addr}");
                result.push(0);
            }
        }
        result
    }

    pub fn write_holding_register(&self, addr: u16, value: u16) {
        if let Some(register) = self.holding_registers.lock().unwrap().get_mut(&addr) {
            *register = value;
        } else {
            warn!("Attempted to write to non-existent holding register {addr}");
        }
    }

    pub fn write_holding_registers(&self, addr: u16, values: &[u16]) {
        let mut registers = self.holding_registers.lock().unwrap();
        for (i, &value) in values.iter().enumerate() {
            let reg_addr = addr + i as u16;
            if let Some(register) = registers.get_mut(&reg_addr) {
                *register = value;
            } else {
                warn!("Attempted to write to non-existent holding register {reg_addr}");
            }
        }
    }
}

pub struct ExampleService {
    shared_state: SharedModbusState,
}

impl tokio_modbus::server::Service for ExampleService {
    type Request = Request<'static>;
    type Response = Response;
    type Exception = ExceptionCode;
    type Future = future::Ready<Result<Self::Response, Self::Exception>>;

    fn call(&self, req: Self::Request) -> Self::Future {
        let res = match req {
            Request::ReadHoldingRegisters(addr, cnt) => {
                let values = self.shared_state.read_holding_registers(addr, cnt);
                Ok(Response::ReadHoldingRegisters(values))
            }
            Request::WriteMultipleRegisters(addr, values) => {
                self.shared_state.write_holding_registers(addr, &values);
                Ok(Response::WriteMultipleRegisters(addr, values.len() as u16))
            }
            Request::WriteSingleRegister(addr, value) => {
                self.shared_state.write_holding_register(addr, value);
                Ok(Response::WriteSingleRegister(addr, value))
            }
            Request::ReadCoils(addr, cnt) => {
                let values = self.shared_state.read_coils(addr, cnt);
                Ok(Response::ReadCoils(values))
            }
            Request::WriteMultipleCoils(addr, values) => {
                self.shared_state.write_coils(addr, &values);
                Ok(Response::WriteMultipleCoils(addr, values.len() as u16))
            }
            Request::WriteSingleCoil(addr, value) => {
                self.shared_state.write_coil(addr, value);
                Ok(Response::WriteSingleCoil(addr, value))
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

    pub fn with_shared_state(shared_state: SharedModbusState) -> Self {
        Self {
            shared_state,
        }
    }
}