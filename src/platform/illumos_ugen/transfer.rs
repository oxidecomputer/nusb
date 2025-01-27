
use std::ffi::c_void;

use crate::transfer::{
    Completion, ControlIn, ControlOut, EndpointType, PlatformSubmit, PlatformTransfer,
    RequestBuffer, ResponseBuffer, TransferError, SETUP_PACKET_SIZE,
};


pub struct TransferData(());

unsafe impl Send for TransferData {}

impl Drop for TransferData {
    fn drop(&mut self) {
        todo!();
    }
}

impl PlatformTransfer for TransferData {
    fn cancel(&self) {
        todo!();
    }
}

impl PlatformSubmit<Vec<u8>> for TransferData {
    unsafe fn submit(&mut self, data: Vec<u8>, user_data: *mut c_void) {
        todo!();
    }

    unsafe fn take_completed(&mut self) -> Completion<ResponseBuffer> {
        todo!();
    }
}

impl PlatformSubmit<RequestBuffer> for TransferData {
    unsafe fn submit(&mut self, data: RequestBuffer, user_data: *mut c_void) {
        todo!();
    }

    unsafe fn take_completed(&mut self) -> Completion<Vec<u8>> {
        todo!();
    }
}

impl PlatformSubmit<ControlIn> for TransferData {
    unsafe fn submit(&mut self, data: ControlIn, user_data: *mut c_void) {
        todo!();
    }

    unsafe fn take_completed(&mut self) -> Completion<Vec<u8>> {
        todo!();
    }
}

impl PlatformSubmit<ControlOut<'_>> for TransferData {
    unsafe fn submit(&mut self, data: ControlOut, user_data: *mut c_void) {
        todo!();
    }

    unsafe fn take_completed(&mut self) -> Completion<ResponseBuffer> {
        todo!();
    }
}


