mod transfer;
use rustix::io::Errno;
pub(crate) use transfer::TransferData;

mod enumeration;
pub use enumeration::{list_buses, list_devices};

mod device;
pub(crate) use device::IllumosDevice as Device;
pub(crate) use device::IllumosInterface as Interface;

mod hotplug;
pub(crate) use hotplug::IllumosHotplugWatch as HotplugWatch;

use crate::transfer::TransferError;

pub type DeviceId = u64;

fn errno_to_transfer_error(e: Errno) -> TransferError {
    match e {
        Errno::NODEV | Errno::SHUTDOWN => TransferError::Disconnected,
        Errno::PIPE => TransferError::Stall,
        Errno::NOENT | Errno::CONNRESET | Errno::TIMEDOUT => TransferError::Cancelled,
        Errno::PROTO | Errno::ILSEQ | Errno::OVERFLOW | Errno::COMM | Errno::TIME => {
            TransferError::Fault
        }
        _ => TransferError::Unknown,
    }
}
