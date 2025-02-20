//! USB Log Reader
//!
//! Looks for device having a logging interface named 'kiffielog'. Then copies
//! all bytes from the endpoint to stdout.
//!
//! The logging interface can have a bulk endpoint or control transfer can be
//! used to retrieve the log data.
//!

use clap::Parser;
use rusb::{Context, Device, DeviceList, Direction, TransferType, UsbContext};
use std::io::Write;
use std::process::exit;
use std::time::Duration;

const INTERFACE_NAME: &str = "kiffielog";
const TIMEOUT: Duration = Duration::from_millis(100);

#[derive(Clone, Copy, Debug)]
enum IfaceType {
    Control,
    Bulk(u8),
}

#[derive(Clone, Debug)]
struct DeviceInfo {
    device: Device<Context>,
    iface_id: u8,
    iface_type: IfaceType,
}

impl DeviceInfo {
    fn control(device: Device<Context>, iface_id: u8) -> Self {
        Self {
            device,
            iface_id,
            iface_type: IfaceType::Control,
        }
    }

    fn bulk(device: Device<Context>, iface_id: u8, ep: u8) -> Self {
        Self {
            device,
            iface_id,
            iface_type: IfaceType::Bulk(ep),
        }
    }

    fn device(&self) -> &Device<Context> {
        &self.device
    }

    fn iface_type(&self) -> IfaceType {
        self.iface_type
    }
}

#[derive(Parser)]
#[command(about = "Reads a USB log channel")]
struct Args {
    /// List devices
    #[clap(short = 'l', long = "list")]
    list: bool,

    /// Select device based on its address
    #[clap(short = 'a', long = "address")]
    address: Option<u8>,

    /// Select device on a given bus
    #[clap(short = 'b', long = "bus")]
    bus: Option<u8>,

    /// Show version information
    #[clap(long = "version")]
    version_info: bool,
}

/// Find devices with log interface
fn find_devices(devices: &'_ DeviceList<Context>) -> impl Iterator<Item = DeviceInfo> + '_ {
    devices
        .iter()
        .filter_map(|dev| dev.open().ok())
        .filter_map(|handle| {
            let dev = handle.device();
            dev.active_config_descriptor().ok().and_then(|conf_desc| {
                conf_desc.interfaces().find_map(|iface| {
                    iface.descriptors().find_map(|if_desc| {
                        if_desc
                            .description_string_index()
                            .and_then(|string_index| {
                                handle.read_string_descriptor_ascii(string_index).ok()
                            })
                            .and_then(|if_name| {
                                (if_name == INTERFACE_NAME).then(|| {
                                    let ep = if_desc.endpoint_descriptors().find(|ep_desc| {
                                        ep_desc.direction() == Direction::In
                                            && ep_desc.transfer_type() == TransferType::Bulk
                                    });
                                    match ep {
                                        Some(ep_desc) => DeviceInfo::bulk(
                                            dev.clone(),
                                            iface.number(),
                                            ep_desc.address(),
                                        ),
                                        None => DeviceInfo::control(dev.clone(), iface.number()),
                                    }
                                })
                            })
                    })
                })
            })
        })
}

fn read_control_log_loop(device_info: &DeviceInfo) -> Result<(), rusb::Error> {
    assert!(matches!(device_info.iface_type(), IfaceType::Control));

    let mut buf = [0; 1024];
    let dev = device_info.device();
    let handle = dev.open()?;
    let iface = device_info.iface_id;
    handle.claim_interface(iface)?;
    let mut stdout = std::io::stdout();
    let bus = dev.bus_number();
    let addr = dev.address();
    let dev_desc = dev.device_descriptor()?;
    let vid = dev_desc.vendor_id();
    let pid = dev_desc.product_id();
    println!(
        "Reading USB log channel from device {vid:04x}:{pid:04x} on bus {bus} at address {addr}"
    );
    loop {
        let request_type = rusb::request_type(
            Direction::In,
            rusb::RequestType::Vendor,
            rusb::Recipient::Interface,
        );
        let res = handle.read_control(request_type, 0, 0, iface as u16, &mut buf, TIMEOUT);
        match res {
            Ok(len) => {
                stdout.write_all(&buf[..len]).unwrap();
            }
            Err(rusb::Error::Timeout) => (),
            Err(e) => {
                eprintln!("Error in Reading from USB: {e}");
                exit(1);
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn read_bulk_log_loop(device_info: &DeviceInfo) -> Result<(), rusb::Error> {
    assert!(matches!(device_info.iface_type, IfaceType::Bulk(_)));

    let dev = device_info.device();
    let handle = dev.open()?;
    let ep = match device_info.iface_type() {
        IfaceType::Bulk(ep) => ep,
        _ => 0,
    };
    handle.claim_interface(device_info.iface_id).unwrap();

    let mut stdout = std::io::stdout();
    let bus = dev.bus_number();
    let addr = dev.address();
    let dev_desc = dev.device_descriptor()?;
    let vid = dev_desc.vendor_id();
    let pid = dev_desc.product_id();
    println!("Reading USB log channel from device {vid:04x}:{pid:04x} on bus {bus} at address {addr}, EP 0x{ep:02x}");
    loop {
        let mut buf = [0; 1024];
        match handle.read_bulk(ep, &mut buf, TIMEOUT) {
            Ok(len) => {
                stdout.write_all(&buf[..len]).unwrap();
            }
            Err(rusb::Error::Timeout) => (),
            Err(e) => {
                eprintln!("Error in Reading from USB: {e}");
                exit(1);
            }
        }
    }
}

fn main() {
    let args: Args = Args::parse();

    if args.version_info {
        println!(
            "{} v{}, ({})",
            env!("CARGO_PKG_NAME"),
            env!("CARGO_PKG_VERSION"),
            env!("BUILD_DATETIME")
        );
        exit(0);
    }

    let context = Context::new().unwrap();
    let device_list = context.devices().unwrap();
    let mut devices: Vec<DeviceInfo> = find_devices(&device_list).collect();

    if args.list {
        for dev_info in devices {
            let dev = dev_info.device();
            let bus = dev.bus_number();
            let addr = dev.address();
            let desc = dev.device_descriptor().unwrap();
            let vid = desc.vendor_id();
            let pid = desc.product_id();
            let handle = dev.open().unwrap();
            let mut names = vec![];
            if let Ok(name) = handle.read_manufacturer_string_ascii(&desc) {
                names.push(name);
            }
            if let Ok(name) = handle.read_product_string_ascii(&desc) {
                names.push(name);
            }
            let names_str = names
                .iter()
                .map(String::from)
                .reduce(|a, b| format!("{a} - {b}"))
                .map(|s| format!(": {s}"))
                .unwrap_or_default();
            println!("Bus {bus:03} Device {addr:03}: {vid:04x}:{pid:04x}{names_str}");
        }
        exit(0);
    }

    if let Some(bus) = args.bus {
        devices.retain(|d| d.device().bus_number() == bus);
    }
    if let Some(addr) = args.address {
        devices.retain(|d| d.device().address() == addr);
    }

    if devices.is_empty() {
        println!("Error: no device found");
        exit(1);
    }
    if devices.len() > 1 {
        println!("Warning: there are multiple log channel interfaces.");
    }
    let selected_device = &devices[0];

    match selected_device.iface_type() {
        IfaceType::Control => read_control_log_loop(selected_device).unwrap(),
        IfaceType::Bulk(_) => read_bulk_log_loop(selected_device).unwrap(),
    }
}
