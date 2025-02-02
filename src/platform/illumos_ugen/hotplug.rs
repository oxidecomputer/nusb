use crate::{hotplug::HotplugEvent, Error};
use std::task::Poll;

pub(crate) struct IllumosHotplugWatch(());

impl IllumosHotplugWatch {
    pub(crate) fn new() -> Result<Self, Error> {
        todo!();
    }

    pub(crate) fn poll_next(&mut self, cx: &mut std::task::Context<'_>) -> Poll<HotplugEvent> {
        todo!();
    }
}
