//! USB Log channel based on control transfers
//!
//! This log channel provides an USB interface without an endpoint. All data
//! transfer is done via control (SETUP) transfers.
//!
// Copyright (C) 2025 Stephan <kiffie@mailbox.org>
// SPDX-License-Identifier: GPL-2.0-or-later

use crate::log_buffer::LogBuffer;
use usb_device::{
    class_prelude::*,
    control::{Recipient, RequestType},
    Result,
};

const INTERFACE_NAME: &str = "kiffielog";
// const XFER_MAX_LEN: usize = 128;
const LOG_READ_REQUEST: u8 = 0;

pub struct UsbLogChannel<'a, const N: usize> {
    iface: InterfaceNumber,
    iface_string: StringIndex,
    log_buffer: &'a LogBuffer<N>,
}

impl<'a, const N: usize> UsbLogChannel<'a, N> {
    /// Create a new USB log channel
    pub fn new<B: UsbBus>(
        alloc: &'a UsbBusAllocator<B>,
        log_buffer: &'a LogBuffer<N>,
    ) -> UsbLogChannel<'a, N> {
        let iface = alloc.interface();
        let iface_string = alloc.string();
        UsbLogChannel {
            iface,
            iface_string,
            log_buffer,
        }
    }
}

impl<B: UsbBus, const N: usize> UsbClass<B> for UsbLogChannel<'_, N> {
    fn get_configuration_descriptors(&self, writer: &mut DescriptorWriter) -> Result<()> {
        writer.interface_alt(self.iface, 0, 0xff, 0, 0, Some(self.iface_string))
    }

    fn get_string(&self, index: StringIndex, _lang_id: LangID) -> Option<&str> {
        if index == self.iface_string {
            Some(INTERFACE_NAME)
        } else {
            None
        }
    }

    fn control_in(&mut self, xfer: ControlIn<B>) {
        let request = xfer.request();
        if request.request_type != RequestType::Vendor
            || request.recipient != Recipient::Interface
            || request.index != Into::<u8>::into(self.iface) as u16
            || request.request != LOG_READ_REQUEST
        {
            return;
        }
        let request_len = request.length as usize;
        xfer.accept(|data| {
            let max_len =  request_len.min(data.len());
            let mut len = 0;
            for d in &mut data[..max_len] {
                if let Some(byte) = self.log_buffer.read() {
                    *d = byte;
                    len += 1;
                } else {
                    break;
                }
            }
            Ok(len)
        }).unwrap();
    }
}
