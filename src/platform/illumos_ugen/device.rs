use crate::{DeviceInfo, Error, Speed};

use rustix::fd::OwnedFd;
use rustix::fs::{Mode, OFlags};
use rustix::io;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use log::warn;

use crate::descriptors::{parse_concatenated_config_descriptors, Configuration, DeviceDescriptor};
use crate::transfer::{
    Control, ControlIn, ControlType, EndpointType, Recipient, TransferError, TransferHandle,
};

//
// Useful USB constants. We use names that deliberately match those found in
// the header files found under usr/src/uts/common/sys/usb.
//
const USB_CFG_DESCR_SIZE: u16 = 9;

const USB_REQ_GET_DESCR: u8 = 0x06;
const USB_REQ_GET_CFG: u8 = 0x08;

pub(crate) struct IllumosDevice {
    fd: OwnedFd,
    device_descriptor: Vec<u8>,
    config_descriptors: Vec<u8>,
    active_config: u8,
}

enum DescriptorType {
    Device,
    Configuration { index: u8 },
    String { index: u8 },
}

impl DescriptorType {
    fn to_value(&self) -> u16 {
        let high_byte = match self {
            DescriptorType::Device => 1,
            DescriptorType::Configuration { .. } => 2,
            DescriptorType::String { .. } => 3,
        } as u16;

        let low_byte = match self {
            DescriptorType::Device => 0,
            DescriptorType::Configuration { index } => *index,
            DescriptorType::String { index } => *index,
        } as u16;

        high_byte << 8 | low_byte
    }
}

fn get_descriptor(fd: &OwnedFd, descriptor_type: DescriptorType) -> Result<Vec<u8>, Error> {
    let wValue: u16 = descriptor_type.to_value();

    let mut control = ControlIn {
        control_type: ControlType::Standard,
        recipient: Recipient::Device,
        request: USB_REQ_GET_DESCR,
        value: wValue,
        index: 0,
        length: USB_CFG_DESCR_SIZE,
    };

    io::write(fd, control.setup_packet().as_slice())?;

    let mut buf = [0u8; USB_CFG_DESCR_SIZE as usize];
    io::read(fd, &mut buf)?;

    let total = u16::from_le_bytes(buf[2..4].try_into().unwrap()) as u16;
    control.length = total;

    io::write(fd, control.setup_packet().as_slice())?;

    let mut descriptors = vec![0u8; total as usize];
    let result = io::read(fd, &mut descriptors);

    Ok(descriptors)
}

fn get_configuration(fd: &OwnedFd) -> Result<u8, Error> {
    let mut control = ControlIn {
        control_type: ControlType::Standard,
        recipient: Recipient::Device,
        request: USB_REQ_GET_CFG,
        value: 0,
        index: 0,
        length: 1,
    };

    let mut buf = [0u8];

    io::write(fd, control.setup_packet().as_slice())?;
    io::read(fd, &mut buf)?;

    Ok(buf[0])
}

impl IllumosDevice {
    pub(crate) fn from_device_info(d: &DeviceInfo) -> Result<Arc<IllumosDevice>, Error> {
        //
        // We are going to open our control FD, and ask for descriptor information.
        // (We expect this information to match that that's already in the devinfo
        // tree as the `usb-raw-cfg-descriptors` property, but we don't cache
        // that in `DeviceInfo`.)
        //
        let path = Path::new(d.path.device_paths.get("cntrl0").unwrap());

        let fd = rustix::fs::open(path, OFlags::RDWR | OFlags::CLOEXEC, Mode::empty())
            .inspect_err(|e| warn!("Failed to open device {path:?}: {e}"))?;

        let device_descriptor = get_descriptor(&fd, DescriptorType::Device)?;
        let config_descriptors = get_descriptor(&fd, DescriptorType::Configuration { index: 0 })?;

        let active_config = get_configuration(&fd)?;

        Ok(Arc::new(Self {
            fd,
            device_descriptor,
            config_descriptors,
            active_config,
        }))
    }

    pub(crate) fn handle_events(&self) {
        todo!();
    }

    pub(crate) fn device_descriptor(&self) -> DeviceDescriptor {
        DeviceDescriptor::new(&self.device_descriptor)
    }

    pub(crate) fn configuration_descriptors(&self) -> impl Iterator<Item = &[u8]> {
        parse_concatenated_config_descriptors(&self.config_descriptors)
    }

    pub(crate) fn active_configuration_value(&self) -> u8 {
        self.active_config
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
        None
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
