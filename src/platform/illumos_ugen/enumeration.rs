use crate::{BusInfo, DeviceInfo, Error};
use crate::descriptors::{
    validate_device_descriptor,
    Configuration,
    DeviceDescriptor
};
use std::collections::HashMap;

#[derive(Debug)]
enum PropVal {
    String(String),
    Bytes(Vec<u8>),
    Integer(i64),
    Boolean,
    Unknown(devinfo::PropType),
}

struct Hub {
    depth: u32,
    port: u8,
}

pub fn list_devices() -> Result<impl Iterator<Item = DeviceInfo>, Error> {
    let mut di = devinfo::DevInfo::new()
        .map_err(|e| Error::new(std::io::ErrorKind::Other, e))?;

    let mut bus = None;
    let mut hubs: Vec<Hub> = Vec::new();

    let mut w = di.walk_node();
    let mut devices = vec![];

    while let Some(n) = w.next().transpose().unwrap() {
        let mut pw = n.props();
        let mut props = HashMap::new();

        while let Some(p) = pw.next().transpose().unwrap() {
            props.insert(p.name(), 
                if let Some(val) = p.as_i64() {
                    PropVal::Integer(val)
                } else if let Some(val) = p.as_bytes() {
                    PropVal::Bytes(val.to_vec())
                } else if let Some(val) = p.to_str() {
                    PropVal::String(val)
                } else {
                    match p.value_type() {
                        devinfo::PropType::Boolean => PropVal::Boolean,
                        t => PropVal::Unknown(t)
                    }
                }
            );
        }

        if let Some(PropVal::Boolean) = props.get("root-hub") {
            bus = match bus {
                Some(bus) => Some(bus + 1),
                None => Some(0),
            };

            hubs = vec![];
            continue;
        }

        let port = match props.get("reg") {
            Some(PropVal::Integer(port)) => *port as u8,
            _ => continue
        };

        let depth = n.depth();

        while hubs.len() > 0 && hubs[hubs.len() - 1].depth >= depth {
            hubs.pop();
        }

        if n.driver_name().as_deref() == Some("hubd") {
            //
            // This is a hub.  Keep track of its assigned address.
            //
            hubs.push(Hub { depth, port });
            continue;
        }

        if let Some(PropVal::Bytes(b)) = props.get("usb-dev-descriptor") {
            let device_address = match props.get("assigned-address") {
                Some(PropVal::Integer(val)) => *val as u8,
                _ => continue
            };

            let manufacturer_string = match props.get("usb-vendor-name") {
                Some(PropVal::String(val)) => Some(val.clone()),
                _ => None 
            };

            let product_string = match props.get("usb-product-name") {
                Some(PropVal::String(val)) => Some(val.clone()),
                _ => None 
            };

            let serial_number = match props.get("usb-serialno") {
                Some(PropVal::String(val)) => Some(val.clone()),
                _ => None 
            };

            let mut port_chain = hubs.iter().map(|h| h.port).collect::<Vec<_>>();
            port_chain.push(port);

            let busnum = match bus {
                Some(bus) => bus,
                None => panic!()
            };

            if let Some(_) = validate_device_descriptor(b) {
                let d = DeviceDescriptor::new(b);

                devices.push(DeviceInfo {
                    bus_id: format!("{busnum:03}"),
                    device_address,
                    port_chain,
                    vendor_id: d.vendor_id(),
                    product_id: d.product_id(),
                    device_version: d.device_version(),
                    class: d.class(),
                    subclass: d.subclass(),
                    protocol: d.protocol(),
                    max_packet_size_0: d.max_packet_size_0(),
                    speed: None,
                    manufacturer_string,
                    product_string,
                    serial_number,
                    interfaces: vec![],
                });
            }
        }
    }

    Ok(devices.into_iter())
}

pub fn list_root_hubs() -> Result<impl Iterator<Item = DeviceInfo>, Error> {
    Ok(std::iter::empty())
}

pub fn list_buses() -> Result<impl Iterator<Item = BusInfo>, Error> {
    Ok(std::iter::empty())
}
