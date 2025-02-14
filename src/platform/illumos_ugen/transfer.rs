use rustix::fd::AsFd;
use rustix::fd::OwnedFd;
use rustix::io;
use rustix::io::Errno;
use std::ffi::c_void;
use std::sync::Arc;

use std::io::{ErrorKind, Seek};

use crate::Error;

use crate::transfer::{
    Completion, ControlIn, ControlOut, EndpointType, PlatformSubmit, PlatformTransfer,
    RequestBuffer, ResponseBuffer, TransferError, SETUP_PACKET_SIZE,
};

pub struct TransferData {
    device: Arc<super::Device>,
    interface: Arc<super::Interface>,
    fd: Arc<OwnedFd>,
    status: Option<Result<usize, Errno>>,
}

unsafe impl Send for TransferData {}

impl TransferData {
    pub(super) fn new(
        device: Arc<super::Device>,
        interface: Option<Arc<super::Interface>>,
        endpoint: u8,
        ep_type: EndpointType,
    ) -> TransferData {
        let binding = interface.unwrap();
        let fd = binding.fds.get(&endpoint).unwrap().clone();

        TransferData {
            interface: binding,
            device: device.clone(),
            fd,
            status: None,
        }
    }
}

impl Drop for TransferData {
    fn drop(&mut self) {}
}

impl PlatformTransfer for TransferData {
    fn cancel(&self) {}
}

impl PlatformSubmit<Vec<u8>> for TransferData {
    unsafe fn submit(&mut self, data: Vec<u8>, user_data: *mut c_void) {
        self.status = Some(io::write(self.fd.as_ref().as_fd(), &data));
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
