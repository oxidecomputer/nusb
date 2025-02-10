use futures_lite::future::block_on;
use nusb::transfer::RequestBuffer;

// STLink commands
const GET_VERSION_EXT: u8 = 0xfb;

const GET_VERSION_EXT_REPLY_SIZE: usize = 12;

// The structure of the return of GET_VERSION_EXT.

// 16 in ST-Link v2 and later (10 in ST-Link v1)
const CMD_SIZE: usize = 16;

// Should really be defined by nusb
const ENDPOINT_IN: u8 = 0x80;
const ENDPOINT_OUT: u8 = 0x00;

// EP is 1 in ST-Link v2 and later (it's 2 in ST-Link v1)
const TX_EP: u8 = 1 | ENDPOINT_OUT;
const RX_EP: u8 = 1 | ENDPOINT_IN;

#[derive(Debug)]
#[repr(C)]
struct VersionExtReply {
    hw_version: u8,
    swim_version: u8,
    jtag_swd_version: u8,
    msc_vcp_version: u8,
    bridge_version: u8,
    power_version: u8,
    vid: u16,
    pid: u16,
}

impl VersionExtReply {
    fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() != GET_VERSION_EXT_REPLY_SIZE {
            return None;
        }

        Some(Self {
            hw_version: bytes[0],
            swim_version: bytes[1],
            jtag_swd_version: bytes[2],
            msc_vcp_version: bytes[3],
            bridge_version: bytes[4],
            power_version: bytes[5],
            vid: u16::from_le_bytes([bytes[8], bytes[9]]),
            pid: u16::from_le_bytes([bytes[10], bytes[11]]),
        })
    }
}

fn main() {
    env_logger::init();
    let di = nusb::list_devices()
        .unwrap()
        .find(|d| d.vendor_id() == 0x0483 && d.product_id() == 0x374e)
        .expect("didn't find an STLink device");

    println!("Device info: {di:#?}");

    let device = di.open().unwrap();
    let interface = device.claim_interface(0).unwrap();

    let mut cmd = vec![0u8; CMD_SIZE];
    cmd[0] = GET_VERSION_EXT;

    block_on(interface.bulk_out(TX_EP, cmd))
        .into_result()
        .unwrap();

    let result = block_on(interface.bulk_in(RX_EP, RequestBuffer::new(256)))
        .into_result()
        .unwrap();

    println!("Raw result: {result:x?}");
    println!("Parsed: {:#x?}", VersionExtReply::from_bytes(&result));
}
