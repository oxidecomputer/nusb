use rustix::fd::AsFd;
use rustix::fd::OwnedFd;
use rustix::io;
use rustix::io::Errno;
use std::ffi::c_void;
use std::sync::Arc;

use super::errno_to_transfer_error;

use crate::transfer::{
    notify_completion, Completion, ControlIn, ControlOut, EndpointType, PlatformSubmit,
    PlatformTransfer, RequestBuffer, ResponseBuffer
};

#[allow(dead_code)]
pub struct TransferData {
    interface: Arc<super::Interface>,
    endpoint: u8,
    device: Arc<super::Device>,
    fd: Arc<OwnedFd>,
    status: Option<Result<usize, Errno>>,
    data: Option<Vec<u8>>,
}

unsafe impl Send for TransferData {}

impl TransferData {
    pub(super) fn new(
        device: Arc<super::Device>,
        interface: Option<Arc<super::Interface>>,
        endpoint: u8,
        _ep_type: EndpointType,
    ) -> TransferData {
        let binding = interface.unwrap();
        let fd = binding.fds.get(&endpoint).unwrap().clone();

        TransferData {
            interface: binding,
            endpoint,
            device: device.clone(),
            fd,
            status: None,
            data: None,
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
        notify_completion::<super::TransferData>(user_data);
    }

    unsafe fn take_completed(&mut self) -> Completion<ResponseBuffer> {
        let (len, status) = match self.status.unwrap() {
            Ok(len) => (len, Ok(())),
            Err(err) => (0, Err(errno_to_transfer_error(err))),
        };

        self.status = None;

        Completion {
            data: ResponseBuffer::from_vec(vec![], len),
            status: status,
        }
    }
}

impl PlatformSubmit<RequestBuffer> for TransferData {
    unsafe fn submit(&mut self, data: RequestBuffer, user_data: *mut c_void) {
        let (mut data, _len) = data.into_vec();
        data.resize(data.capacity(), 0);

        self.status = Some(io::read(self.fd.as_ref().as_fd(), &mut data));
        self.data = Some(data);

        notify_completion::<super::TransferData>(user_data);
    }

    unsafe fn take_completed(&mut self) -> Completion<Vec<u8>> {
        let (len, status) = match self.status.unwrap() {
            Ok(len) => (len, Ok(())),
            Err(err) => (0, Err(errno_to_transfer_error(err))),
        };

        Completion {
            data: self.data.take().unwrap()[0..len].to_vec(),
            status,
        }
    }
}

impl PlatformSubmit<ControlIn> for TransferData {
    #[allow(unused)]
    unsafe fn submit(&mut self, data: ControlIn, user_data: *mut c_void) {
        todo!();
    }

    unsafe fn take_completed(&mut self) -> Completion<Vec<u8>> {
        todo!();
    }
}

impl PlatformSubmit<ControlOut<'_>> for TransferData {
    #[allow(unused)]
    unsafe fn submit(&mut self, data: ControlOut, user_data: *mut c_void) {
        todo!();
    }

    unsafe fn take_completed(&mut self) -> Completion<ResponseBuffer> {
        todo!();
    }
}
