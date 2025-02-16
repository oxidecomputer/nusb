use crate::descriptors::{
    validate_config_descriptor, validate_device_descriptor, Configuration, DeviceDescriptor,
};
use crate::{BusInfo, DeviceInfo, Error, InterfaceInfo};
use std::collections::HashMap;

use anyhow::{anyhow, bail};

#[allow(dead_code)]
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

#[derive(Clone, Debug)]
pub struct DevfsPath {
    pub path: String,
    pub device_paths: HashMap<String, String>,
}

fn walk_devices() -> Result<Vec<DeviceInfo>, anyhow::Error> {
    let mut di = devinfo::DevInfo::new()?;
    let mut bus = None;
    let mut hubs: Vec<Hub> = Vec::new();

    let mut w = di.walk_node();
    let mut devices = vec![];

    let links = devinfo::DevLinks::new(false)?;

    while let Some(n) = w.next().transpose().unwrap() {
        let mut pw = n.props();
        let mut props = HashMap::new();
        let path = n.devfs_path()?;

        while let Some(p) = pw.next().transpose().unwrap() {
            props.insert(
                p.name(),
                if let Some(val) = p.as_i64() {
                    PropVal::Integer(val)
                } else if let Some(val) = p.as_bytes() {
                    PropVal::Bytes(val.to_vec())
                } else if let Some(val) = p.to_str() {
                    PropVal::String(val)
                } else {
                    match p.value_type() {
                        devinfo::PropType::Boolean => PropVal::Boolean,
                        t => PropVal::Unknown(t),
                    }
                },
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
            None => continue,
            m => bail!("{path:?}: unexpected type for reg: {m:?}"),
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
                Some(PropVal::Integer(v)) if *v < u8::MAX.into() => *v as u8,
                v => bail!("{path:?}: bad assigned-address: {v:?}"),
            };

            let manufacturer_string = match props.get("usb-vendor-name") {
                Some(PropVal::String(v)) => Some(v.clone()),
                None => continue,
                v => bail!("{path}: bad usb-vendor-name: {v:?}"),
            };

            let product_string = match props.get("usb-product-name") {
                Some(PropVal::String(val)) => Some(val.clone()),
                None => None,
                v => bail!("{path}: bad usb-product-name: {v:?}"),
            };

            let serial_number = match props.get("usb-serialno") {
                Some(PropVal::String(val)) => Some(val.clone()),
                None => None,
                v => bail!("{path}: bad usb-serialno: {v:?}"),
            };

            let mut ports = hubs.iter().map(|h| h.port).collect::<Vec<_>>();
            ports.push(port);

            let busnum = match bus {
                Some(bus) => bus,
                None => bail!("{path}: no root port?"),
            };

            if let Some(_) = validate_device_descriptor(b) {
                let d = DeviceDescriptor::new(b);

                let interfaces = match props.get("usb-raw-cfg-descriptors") {
                    Some(PropVal::Bytes(cfg)) => {
                        #[rustfmt::skip]
                        validate_config_descriptor(cfg).ok_or_else(||
                            anyhow!("{path}: bad config {cfg:?}")
                        )?;

                        let c = Configuration::new(cfg);

                        c.interfaces()
                            .map(|i| {
                                let alt = i.first_alt_setting();

                                //
                                // If we want to pull the interface string,
                                // we'll need to open the configuration
                                // endpoint and pull the String descriptors.
                                //
                                InterfaceInfo {
                                    interface_number: i.interface_number(),
                                    class: alt.class(),
                                    subclass: alt.subclass(),
                                    protocol: alt.protocol(),
                                    interface_string: None,
                                }
                            })
                            .collect::<Vec<_>>()
                    }
                    v => bail!("{path}: bad usb-raw-cfg-descriptors: {v:?}"),
                };

                let mut paths: HashMap<String, String> = HashMap::new();

                let mut wm = n.minors();
                while let Some(m) = wm.next().transpose()? {
                    let minor_path = m.devfs_path()?;

                    for link in links.links_for_path(minor_path)? {
                        let lpath = link.path();

                        let file_name = match lpath.file_name() {
                            Some(file_name) => file_name.to_str().unwrap(),
                            None => bail!("{path}: bad link path {lpath:?}"),
                        };

                        #[rustfmt::skip]
                        paths.insert(
                            file_name.to_string(),
                            lpath.to_string_lossy().into_owned()
                        );
                    }
                }

                devices.push(DeviceInfo {
                    path: DevfsPath {
                        path,
                        device_paths: paths,
                    },
                    bus_id: format!("{busnum:03}"),
                    device_address,
                    port_chain: ports,
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
                    interfaces,
                });
            } else {
                bail!("{path}: invalid device descriptor");
            }
        }
    }

    Ok(devices)
}

pub fn list_devices() -> Result<impl Iterator<Item = DeviceInfo>, Error> {
    #[rustfmt::skip]
    let devices = walk_devices()
        .map_err(|e| Error::new(std::io::ErrorKind::Other, e))?;

    Ok(devices.into_iter())
}

pub fn list_buses() -> Result<impl Iterator<Item = BusInfo>, Error> {
    todo!();
    #[allow(unreachable_code)]
    Ok(std::iter::empty())
}
