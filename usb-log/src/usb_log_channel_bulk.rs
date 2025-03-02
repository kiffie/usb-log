//! USB Log channel
//!
//! This log channel provides an USB interface having one bulk IN endpoint The
//! interface can be labelled with a specific string so that a libusb based log
//! client can identify the interface and the respective USB endpoint
//!
// Copyright (C) 2022 Stephan <kiffie@mailbox.org>
// SPDX-License-Identifier: GPL-2.0-or-later

use crate::log_buffer::LogBuffer;
use usb_device::{class_prelude::*, Result};

const EP_SIZE: usize = 64;

const INTERFACE_NAME: &str = "kiffielog";

pub struct UsbLogChannel<'a, B: UsbBus, const N: usize> {
    iface: InterfaceNumber,
    iface_string: StringIndex,
    ep_in: EndpointIn<'a, B>,
    log_buffer: &'a LogBuffer<N>,
    packet_buffer: [u8; EP_SIZE],
    packet_buffer_len: usize,
}

impl<'a, B: UsbBus, const N: usize> UsbLogChannel<'a, B, N> {

    /// Create a new USB log channel
    pub fn new(
        alloc: &'a UsbBusAllocator<B>,
        log_buffer: &'a LogBuffer<N>,
    ) -> UsbLogChannel<'a, B, N> {
        let iface = alloc.interface();
        let iface_string = alloc.string();
        let ep_in = alloc.bulk(EP_SIZE as u16);
        let packet_buffer = [0; EP_SIZE];
        let packet_buffer_len = 1;
        UsbLogChannel {
            iface,
            iface_string,
            ep_in,
            log_buffer,
            packet_buffer,
            packet_buffer_len,
        }
    }

    /// Periodic tasks.
    ///
    /// his needs to be called periodically to process the log messages.
    pub fn tasks(&mut self) {
        self.poll();
    }

}

impl<B: UsbBus, const N: usize> UsbClass<B> for UsbLogChannel<'_, B, N> {
    fn get_configuration_descriptors(&self, writer: &mut DescriptorWriter) -> Result<()> {
        writer.interface_alt(self.iface, 0, 0xff, 0, 0, Some(self.iface_string))?;
        writer.endpoint(&self.ep_in)
    }

    fn get_string(&self, index: StringIndex, _lang_id: LangID) -> Option<&str> {
        if index == self.iface_string {
            Some(INTERFACE_NAME)
        } else {
            None
        }
    }

    fn poll(&mut self) {
        if self.packet_buffer_len == 0 {
            while let Some(byte) = self.log_buffer.read() {
                self.packet_buffer[self.packet_buffer_len] = byte;
                self.packet_buffer_len += 1;
                if self.packet_buffer_len >= EP_SIZE - 1 {
                    break;
                }
            }
        }
        if self.packet_buffer_len > 0
            && self
                .ep_in
                .write(&self.packet_buffer[..self.packet_buffer_len])
                .is_ok()
        {
            self.packet_buffer_len = 0;
        }
    }
}
