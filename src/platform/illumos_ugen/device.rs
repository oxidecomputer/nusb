use crate::{DeviceInfo, Error, Speed};

use rustix::fd::OwnedFd;
use std::sync::Arc;
use std::time::Duration;

use crate::descriptors::{validate_device_descriptor, Configuration, DeviceDescriptor};
use crate::transfer::{Control, EndpointType, TransferError, TransferHandle};

pub(crate) struct IllumosDevice {
    fd: OwnedFd,
}

impl IllumosDevice {
    pub(crate) fn from_device_info(d: &DeviceInfo) -> Result<Arc<IllumosDevice>, Error> {
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
