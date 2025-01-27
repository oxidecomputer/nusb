use crate::{BusInfo, DeviceInfo, Error};

pub fn list_devices() -> Result<impl Iterator<Item = DeviceInfo>, Error> {
    Ok(std::iter::empty())
}

pub fn list_root_hubs() -> Result<impl Iterator<Item = DeviceInfo>, Error> {
    Ok(std::iter::empty())
}

pub fn list_buses() -> Result<impl Iterator<Item = BusInfo>, Error> {
    Ok(std::iter::empty())
}
