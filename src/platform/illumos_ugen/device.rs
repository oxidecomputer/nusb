use crate::{DeviceInfo, Error, Speed};

use rustix::fd::OwnedFd;
use std::sync::Arc;
use std::time::Duration;
use std::path::Path;
use rustix::fs::{Mode, OFlags};
use rustix::io;

use log::warn;

use crate::descriptors::{validate_device_descriptor, Configuration, DeviceDescriptor};
use crate::transfer::{Control, ControlIn, ControlType, EndpointType, Recipient, TransferError, TransferHandle};

pub(crate) struct IllumosDevice {
    fd: OwnedFd,
}

impl IllumosDevice {
    pub(crate) fn from_device_info(d: &DeviceInfo) -> Result<Arc<IllumosDevice>, Error> {
        let path = Path::new(d.path.device_paths.get("cntrl0").unwrap());

        const USB_REQ_GET_DESCR: u8 = 0x06;
        const USB_DESCR_TYPE_SETUP_CFG: u16 = 0x200;
        const USB_CFG_DESCR_SIZE: u16 = 9;

        let mut control = ControlIn {
            control_type: ControlType::Standard,
            recipient: Recipient::Device,
            request: USB_REQ_GET_DESCR,
            value: USB_DESCR_TYPE_SETUP_CFG,
            index: 0,
            length: USB_CFG_DESCR_SIZE,
        };

        let fd = rustix::fs::open(path, OFlags::RDWR | OFlags::CLOEXEC, Mode::empty())
            .inspect_err(|e| warn!("Failed to open device {path:?}: {e}"))?;

        io::write(&fd, control.setup_packet().as_slice())?;

        let mut buf = [0u8; USB_CFG_DESCR_SIZE as usize];

        io::read(&fd, &mut buf)?;

        let total = u16::from_le_bytes(buf[2..4].try_into().unwrap()) as u16;
        control.length = total;

        io::write(&fd, control.setup_packet().as_slice())?;

        let mut descriptors = vec![0u8; total as usize];
        let result = io::read(&fd, &mut descriptors);

        println!("result of read 2 is {:?}, buffer is {:x?}", result, buf);

        /*
        if let Some(_) = validate_device_descriptor(&descriptor) {
            let d = DeviceDescriptor::new(&descriptor);
            println!("{:#x?}", d);
        } else {
            println!("uh oh");
        }
        */

        // We are going to open our control FD, and ask for descriptor information
        
        todo!();
    }
    pub(crate) fn handle_events(&self) {
        todo!();
    }

    pub(crate) fn device_descriptor(&self) -> DeviceDescriptor {
        todo!();
    }

    pub(crate) fn configuration_descriptors(&self) -> impl Iterator<Item = &[u8]> {
        std::iter::empty()
    }

    pub(crate) fn active_configuration_value(&self) -> u8 {
        todo!();
    }

    pub(crate) fn set_configuration(&self, configuration: u8) -> Result<(), Error> {
        todo!();
    }

    pub(crate) fn reset(&self) -> Result<(), Error> {
        todo!();
    }

    pub fn control_in_blocking(
        &self,
        control: Control,
        data: &mut [u8],
        timeout: Duration,
    ) -> Result<usize, TransferError> {
        todo!();
    }

    pub fn control_out_blocking(
        &self,
        control: Control,
        data: &[u8],
        timeout: Duration,
    ) -> Result<usize, TransferError> {
        todo!();
    }

    pub(crate) fn make_control_transfer(self: &Arc<Self>) -> TransferHandle<super::TransferData> {
        todo!();
    }

    pub(crate) fn claim_interface(
        self: &Arc<Self>,
        interface_number: u8,
    ) -> Result<Arc<IllumosInterface>, Error> {
        todo!();
    }

    pub(crate) fn detach_and_claim_interface(
        self: &Arc<Self>,
        interface_number: u8,
    ) -> Result<Arc<IllumosInterface>, Error> {
        todo!();
    }

    pub(crate) fn speed(&self) -> Option<Speed> {
        todo!();
    }
}

impl Drop for IllumosDevice {
    fn drop(&mut self) {
        todo!();
    }
}

pub(crate) struct IllumosInterface {
    pub(crate) interface_number: u8,
    pub(crate) device: Arc<IllumosDevice>,
}

impl IllumosInterface {
    pub(crate) fn make_transfer(
        self: &Arc<Self>,
        endpoint: u8,
        ep_type: EndpointType,
    ) -> TransferHandle<super::TransferData> {
        todo!();
    }

    pub fn control_in_blocking(
        &self,
        control: Control,
        data: &mut [u8],
        timeout: Duration,
    ) -> Result<usize, TransferError> {
        todo!();
    }

    pub fn control_out_blocking(
        &self,
        control: Control,
        data: &[u8],
        timeout: Duration,
    ) -> Result<usize, TransferError> {
        todo!();
    }

    pub fn set_alt_setting(&self, alt_setting: u8) -> Result<(), Error> {
        todo!();
    }

    pub fn clear_halt(&self, endpoint: u8) -> Result<(), Error> {
        todo!();
    }
}

impl Drop for IllumosInterface {
    fn drop(&mut self) {
        todo!();
    }
}
