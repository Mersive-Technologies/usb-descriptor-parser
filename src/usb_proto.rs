// derived from /usr/include/linux/usb/ch8.h
#![allow(dead_code)]

use std::io::Write;

use anyhow::Error;
use libusb1_sys::constants::*;
use structure::byteorder::WriteBytesExt;
use std::hash::Hash;
use std::fmt::{Display, Debug, Formatter};
use crate::uac_proto::{UacInterfaceSubclass, UacInputTerminalDescriptor, Uac1OutputTerminalDescriptor, UacFeatureUnitDescriptor, Uac1AsHeaderDescriptor, UacFormatTypeIContinuousDescriptor, DescriptorUacFormatTypeUnknown, UacIsoEndpointDescriptor, Uac1AcHeaderDescriptor, DescriptorUacInterfaceUnknown};
use crate::uvc_proto::{UvcInterfaceSubClass, DescriptorUvcInputHeader, DescriptorUvcFormatUncompressed, DescriptorUvcFormatMjpeg, DescriptorUvcFrameUncompressed, DescriptorUvcFrameMjpeg, DescriptorUvcFormatFrameBased, DescriptorUvcFrameFrameBased, DescriptorUvcVsInterfaceUnknown, DescriptorUvcVcInterfaceUnknown, UvcHeaderDescriptor, UvcInputTerminalDescriptor, UvcProcessingUnitDescriptor, UvcExtensionUnitDescriptor, UvcOutputTerminalDescriptor};

pub const MERSIVE_VID: u16 = 0x326e;

// 9.3 USB Device Requests
#[derive(Debug, Clone, Copy, FromPrimitive, PartialEq)]
#[repr(u8)]
pub enum XferDir {
    ToDev = 0x00,
    ToHost = 0x80,
}
pub const USB_DIR_MASK: u8 = 0x1 << 7;

#[derive(Debug, Clone, Copy, FromPrimitive, PartialEq)]
#[repr(u8)]
pub enum XferType {
    Std = 0x00,
    Class = 0x20,
    Vendor = 0x40,
    Reserved = 0x60,
}
pub const USB_XFER_TYPE_MASK: u8 = 0x03 << 5;

#[derive(Debug, Clone, Copy, FromPrimitive, PartialEq)]
#[repr(u8)]
pub enum Recip {
    Dev = 0x00,
    Iface = 0x01,
    Ep = 0x02,
    Other = 0x03,
    Reserved4 = 4,
    Reserved5 = 5,
    Reserved6 = 6,
    Reserved7 = 7,
    Reserved8 = 8,
    Reserved9 = 9,
    Reserved10 = 10,
    Reserved11 = 11,
    Reserved12 = 12,
    Reserved13 = 13,
    Reserved14 = 14,
    Reserved15 = 15,
    Reserved16 = 16,
    Reserved17 = 17,
    Reserved18 = 18,
    Reserved19 = 19,
    Reserved20 = 20,
    Reserved21 = 21,
    Reserved22 = 22,
    Reserved23 = 23,
    Reserved24 = 24,
    Reserved25 = 25,
    Reserved26 = 26,
    Reserved27 = 27,
    Reserved28 = 28,
    Reserved29 = 29,
    Reserved30 = 30,
    Reserved31 = 31,
}
pub const USB_RECIP_MASK: u8 = 0x1f;

#[derive(Debug, Clone, Copy, FromPrimitive)]
#[repr(u8)]
pub enum UsbDescriptorTypes {
    Device = 0x01,
    Config = 0x02,
    String = 0x03,
    Interface = 0x04,
    Endpoint = 0x05,
    DeviceQualifier = 0x06,
    OtherSpeedConfig = 0x07,
    InterfacePower = 0x08,
    Otg = 0x09,
    Debug = 0x0a,
    InterfaceAssociation = 0x0b,
    Security = 0x0c,
    Key = 0x0d,
    EncryptionType = 0x0e,
    Bos = 0x0f,
    DeviceCapability = 0x10,
    WirelessEndpointComp = 0x11,
    CsDevice = 0x21,
    CsConfig = 0x22,
    CsString = 0x23,
    CsInterface = 0x24,
    CsEndpoint = 0x25,
    SuperSpeedEpComp = 0x30,
    SuperSpeedPlusIsochEpComp = 0x31,
}

// USB
// http://sdphca.ucsd.edu/lab_equip_manuals/usb_20.pdf
// https://github.com/torvalds/linux/blob/master/include/uapi/linux/usb/ch9.h

#[derive(Debug, Clone, Copy, Eq, Hash, PartialEq)]
pub struct DeviceAddress {
    pub bus: u8,
    pub num: u8,
}

impl DeviceAddress {
    pub fn new(bus: u8, num: u8) -> DeviceAddress {
        DeviceAddress { bus, num }
    }
}

impl Display for DeviceAddress {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}", self.bus, self.num)
    }
}

#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub struct InterfaceAddress {
    pub bus: u8,
    pub num: u8,
    pub iface: u8,
}

impl InterfaceAddress {
    pub fn new(bus: u8, num: u8, iface: u8) -> InterfaceAddress {
        InterfaceAddress { bus, num, iface }
    }
}

impl Debug for InterfaceAddress {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.bus, self.num, self.iface)
    }
}

#[derive(Debug, Clone, Copy, Eq, Hash, PartialEq)]
pub struct EndpointAddress {
    pub bus: u8,
    pub num: u8,
    pub ep: u8,
}

impl EndpointAddress {
    pub fn new(bus: u8, num: u8, ep: u8) -> EndpointAddress {
        EndpointAddress {bus, num, ep}
    }

    pub fn invalid() -> EndpointAddress {
        EndpointAddress::new(0, 0, 0)
    }

    pub fn ep_addr(&self) -> u8 {
        self.ep & LIBUSB_ENDPOINT_ADDRESS_MASK
    }

    pub fn dir(&self) -> u8 {
        self.ep & LIBUSB_ENDPOINT_DIR_MASK
    }

    pub fn dir_name(&self) -> String {
        let dname = if self.dir() == LIBUSB_ENDPOINT_IN { "IN" } else { "OUT" };
        dname.to_string()
    }
}

impl Display for EndpointAddress {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{} dir {}", self.bus, self.num, self.ep_addr(), self.dir_name())
    }
}

#[derive(Debug, Clone, Copy, Eq, Hash, PartialEq)]
pub struct IfaceAltSetting {
    pub iface: u8,
    pub alt: u8,
}

impl IfaceAltSetting {
    pub fn new(iface: u8, alt_setting: u8) -> IfaceAltSetting {
        IfaceAltSetting {iface, alt: alt_setting }
    }

    pub fn invalid() -> IfaceAltSetting {
        IfaceAltSetting::new(0, 0)
    }
}

impl Display for IfaceAltSetting {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}", self.iface, self.alt)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct UsbDescriptorHeader {
    pub b_length: u8,
    pub b_descriptor_type: u8,
}

impl UsbDescriptorHeader {
    pub fn deserialize(mut buffer: &mut &[u8]) -> UsbDescriptorHeader {
        let format = structure!("<BB");
        let (b_length, b_descriptor_type) = format.unpack_from(&mut buffer).unwrap();
        let msg = UsbDescriptorHeader { b_length, b_descriptor_type };
        return msg;
    }
}

#[derive(Debug, Clone)]
pub struct DescriptorUnknown {
    pub desc_type: u8,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct DescriptorCsDevice {
    pub bytes: Vec<u8>,
}

impl DescriptorCsDevice {
    pub fn serialize(&self, mut buffer: impl Write) {
        buffer.write_u8(self.bytes.len() as u8 + 2u8).unwrap();
        buffer.write_u8(UsbDescriptorTypes::CsDevice as u8).unwrap();
        buffer.write_all(&self.bytes).unwrap();
    }
}

#[derive(Debug, Clone)]
pub struct DescriptorCsInterface {
    pub bytes: Vec<u8>,
}

impl DescriptorCsInterface {
    pub fn serialize(&self, mut buffer: impl Write) {
        buffer.write_u8(self.bytes.len() as u8 + 2u8).unwrap();
        buffer.write_u8(UsbDescriptorTypes::CsInterface as u8).unwrap();
        buffer.write_all(&self.bytes).unwrap();
    }
}

#[derive(Debug, Clone)]
pub struct DescriptorCsEndpoint {
    pub bytes: Vec<u8>,
}

impl DescriptorCsEndpoint {
    pub fn serialize(&self, mut buffer: impl Write) {
        buffer.write_u8(self.bytes.len() as u8 + 2u8).unwrap();
        buffer.write_u8(UsbDescriptorTypes::CsEndpoint as u8).unwrap();
        buffer.write_all(&self.bytes).unwrap();
    }
}

#[derive(Debug, Clone, Copy)]
pub struct DescriptorDevice {
    pub bcd_usb: u16,
    pub b_device_class: u8,
    pub b_device_sub_class: u8,
    pub b_device_protocol: u8,
    pub b_max_packet_size0: u8,
    pub id_vendor: u16,
    pub id_product: u16,
    pub bcd_device: u16,
    pub i_manufacturer: u8,
    pub i_product: u8,
    pub i_serial_number: u8,
    pub b_num_configurations: u8,
}

impl DescriptorDevice {
    pub fn serialize(&self, mut buffer: impl Write) {
        let format = structure!("<BBHBBBBHHHBBBB");
        format.pack_into(&mut buffer, format.size() as u8, UsbDescriptorTypes::Device as u8,
                         self.bcd_usb, self.b_device_class, self.b_device_sub_class, self.b_device_protocol, self.b_max_packet_size0, self.id_vendor, self.id_product, self.bcd_device, self.i_manufacturer, self.i_product, self.i_serial_number, self.b_num_configurations,
        ).unwrap();
    }
    pub fn deserialize(buffer: &mut &[u8]) -> Result<DescriptorDevice, Error> {
        let format = structure!("<HBBBBHHHBBBB");
        let (bcd_usb, b_device_class, b_device_sub_class, b_device_protocol, b_max_packet_size0, id_vendor, id_product, bcd_device, i_manufacturer, i_product, i_serial_number, b_num_configurations, ) = format.unpack_from(buffer)?;
        let msg = DescriptorDevice { bcd_usb, b_device_class, b_device_sub_class, b_device_protocol, b_max_packet_size0, id_vendor, id_product, bcd_device, i_manufacturer, i_product, i_serial_number, b_num_configurations };
        Ok(msg)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct DescriptorDevQualifier {
    pub bcd_usb: u16,
    pub b_device_class: u8,
    pub b_device_sub_class: u8,
    pub b_device_protocol: u8,
    pub b_max_packet_size0: u8,
    pub b_num_configurations: u8,
    pub b_reserved: u8,
}

impl DescriptorDevQualifier {
    pub fn serialize(&self, mut buffer: impl Write) {
        let format = structure!("<BBHBBBBBB");
        format.pack_into(&mut buffer, format.size() as u8, UsbDescriptorTypes::DeviceQualifier as u8,
                         self.bcd_usb, self.b_device_class, self.b_device_sub_class, self.b_device_protocol,
                         self.b_max_packet_size0, self.b_num_configurations, self.b_reserved,
        ).unwrap();
    }
    pub fn deserialize(mut buffer: &mut &[u8]) -> DescriptorDevQualifier {
        let format = structure!("<HBBBBBB");
        let (
            bcd_usb, b_device_class, b_device_sub_class, b_device_protocol, b_max_packet_size0, b_num_configurations, b_reserved
        ) = format.unpack_from(&mut buffer).unwrap();
        let msg = DescriptorDevQualifier {
            bcd_usb, b_device_class, b_device_sub_class, b_device_protocol, b_max_packet_size0, b_num_configurations, b_reserved
        };
        return msg;
    }
}

#[derive(Debug, Clone, Copy)]
pub struct DescriptorConfig {
    pub w_total_length: u16,
    pub b_num_interfaces: u8,
    pub b_configuration_value: u8,
    pub i_configuration: u8,
    pub bm_attributes: u8,
    pub b_max_power: u8,
}

impl DescriptorConfig {
    pub fn serialize(&self, mut buffer: impl Write) {
        let format = structure!("<BBHBBBBB");
        format.pack_into(&mut buffer, format.size() as u8, UsbDescriptorTypes::Config as u8, self.w_total_length, self.b_num_interfaces, self.b_configuration_value, self.i_configuration,
                         self.bm_attributes, self.b_max_power,
        ).unwrap();
    }
    pub fn deserialize(mut buffer: &mut &[u8]) -> DescriptorConfig {
        let format = structure!("<HBBBBB");
        let (w_total_length, b_num_interfaces, b_configuration_value, i_configuration, bm_attributes, b_max_power) = format.unpack_from(&mut buffer).unwrap();
        let msg = DescriptorConfig { w_total_length, b_num_interfaces, b_configuration_value, i_configuration, bm_attributes, b_max_power };
        return msg;
    }
}

#[derive(Debug, Clone, Copy)]
pub struct DescriptorInterface {
    pub b_interface_number: u8,
    pub b_alternate_setting: u8,
    pub b_num_endpoints: u8,
    pub b_interface_class: u8,
    pub b_interface_sub_class: u8,
    pub b_interface_protocol: u8,
    pub i_interface: u8,
}

impl DescriptorInterface {
    pub fn is_audio(&self) -> bool {
        self.b_interface_class == LIBUSB_CLASS_AUDIO
    }
    pub fn is_audio_control(&self) -> bool {
        self.is_audio() && self.b_interface_sub_class == UacInterfaceSubclass::AudioControl as u8
    }
    pub fn is_audio_streaming(&self) -> bool {
        self.is_audio() && self.b_interface_sub_class == UacInterfaceSubclass::AudioStreaming as u8
    }
    pub fn is_video_streaming(&self) -> bool {
        self.b_interface_class == LIBUSB_CLASS_VIDEO && self.b_interface_sub_class == UvcInterfaceSubClass::VideoStreaming as u8
    }
    pub fn serialize(&self, mut buffer: impl Write) {
        let format = structure!("<BBBBBBBBB");
        format.pack_into(&mut buffer, format.size() as u8, UsbDescriptorTypes::Interface as u8, self.b_interface_number, self.b_alternate_setting, self.b_num_endpoints, self.b_interface_class,
                         self.b_interface_sub_class, self.b_interface_protocol, self.i_interface,
        ).unwrap();
    }
    pub fn deserialize(mut buffer: &mut &[u8]) -> DescriptorInterface {
        let format = structure!("<BBBBBBB");
        let (b_interface_number, b_alternate_setting, b_num_endpoints, b_interface_class, b_interface_sub_class, b_interface_protocol, i_interface) = format.unpack_from(&mut buffer).unwrap();
        let msg = DescriptorInterface { b_interface_number, b_alternate_setting, b_num_endpoints, b_interface_class, b_interface_sub_class, b_interface_protocol, i_interface };
        return msg;
    }
}

#[derive(Debug, Clone, Copy)]
pub struct UsbInterfaceAssocDescriptor {
    pub b_first_interface: u8,
    pub b_interface_count: u8,
    pub b_function_class: u8,
    pub b_function_sub_class: u8,
    pub b_function_protocol: u8,
    pub i_function: u8,
}

impl UsbInterfaceAssocDescriptor {
    pub fn serialize(&self, mut buffer: impl Write) {
        let format = structure!("<BBBBBBBB");
        format.pack_into(&mut buffer, format.size() as u8, UsbDescriptorTypes::InterfaceAssociation as u8, self.b_first_interface, self.b_interface_count, self.b_function_class,
                         self.b_function_sub_class, self.b_function_protocol, self.i_function,
        ).unwrap();
    }
    pub fn deserialize(mut buffer: &mut &[u8]) -> UsbInterfaceAssocDescriptor {
        let format = structure!("<BBBBBB");
        let (b_first_interface, b_interface_count, b_function_class, b_function_sub_class, b_function_protocol, i_function) = format.unpack_from(&mut buffer).unwrap();
        let msg = UsbInterfaceAssocDescriptor { b_first_interface, b_interface_count, b_function_class, b_function_sub_class, b_function_protocol, i_function };
        return msg;
    }
    pub fn last_iface(&self) -> u8 {
        self.b_first_interface + self.b_interface_count - 1
    }
}

#[derive(Debug, Clone, Copy)]
pub enum DescriptorTransferType {
    Control,
    Isochronous,
    Bulk,
    Interrupt,
    BulkStream,
    Invalid,
}

#[derive(Debug, Clone, Copy)]
pub enum SynchType {
    None,
    Asynchronous,
    Adaptive,
    Synchronous,
    Invalid,
}

#[derive(Debug, Clone, Copy)]
pub enum UsageType {
    Data,
    Feedback,
    Implicit,
    Invalid,
}

#[derive(Debug, Clone, Copy)]
pub struct UsbSsEpCompDescriptor {
    b_max_burst: u8,
    bm_attributes: u8,
    w_bytes_per_interval: u16,
}
impl UsbSsEpCompDescriptor {
    pub fn serialize(&self, mut buffer: impl Write) {
        let format = structure!("<BBBBH");
        format.pack_into(&mut buffer,
                         self.size() as u8, UsbDescriptorTypes::SuperSpeedEpComp as u8,
                         self.b_max_burst, self.bm_attributes, self.w_bytes_per_interval).unwrap();
    }
    pub fn deserialize(mut buffer: &mut &[u8]) -> UsbSsEpCompDescriptor {
        let format = structure!("<BBH");
        let (b_max_burst, bm_attributes, w_bytes_per_interval) = format.unpack_from(&mut buffer).unwrap();
        let msg = UsbSsEpCompDescriptor {
            b_max_burst, bm_attributes, w_bytes_per_interval
        };
        return msg;
    }
    pub fn size(&self) -> usize {
        structure!("<BBBBH").size()
    }
    pub fn max_burst(&self) -> u8 { self.b_max_burst + 1 }
    pub fn mult(&self) -> u8 { (self.bm_attributes & 0x03) + 1 }
}

#[derive(Debug, Clone, Copy)]
pub struct UsbSspIsochEpCompDescriptor {
    w_reserved: u16,
    dw_bytes_per_interval: u32,
}
impl UsbSspIsochEpCompDescriptor {
    pub fn serialize(&self, mut buffer: impl Write) {
        let format = structure!("<BBHI");
        format.pack_into(&mut buffer,
                         self.size() as u8, UsbDescriptorTypes::SuperSpeedPlusIsochEpComp as u8,
                         self.w_reserved, self.dw_bytes_per_interval).unwrap();
    }
    pub fn deserialize(mut buffer: &mut &[u8]) -> UsbSspIsochEpCompDescriptor {
        let format = structure!("<HI");
        let (w_reserved, dw_bytes_per_interval) = format.unpack_from(&mut buffer).unwrap();
        let msg = UsbSspIsochEpCompDescriptor { w_reserved, dw_bytes_per_interval };
        return msg;
    }
    pub fn size(&self) -> usize {
        structure!("<BBHI").size()
    }
}

#[derive(Debug, Clone, Copy)]
pub enum EndpointAttrTransferType {
    Control     = 0x00,
    Isochronous = 0x01,
    Bulk        = 0x02,
    Interrupt   = 0x03,
}

#[derive(Debug, Clone, Copy)]
pub enum EndpointAttrSyncType {
    NoSync  = 0x00,
    Async   = 0x01,
    Adapt   = 0x02,
    Sync    = 0x03,
}

#[derive(Debug, Clone, Copy)]
pub enum EndpointAttrUsageType {
    Data        = 0x00,
    Feedback    = 0x01,
    Implicit    = 0x02,
    Reserved    = 0x03,
}

pub fn ep_attr_to_u8(transfer: EndpointAttrTransferType, sync: EndpointAttrSyncType, usage: EndpointAttrUsageType) -> u8 {
    let mut res = 0u8;
    res |= transfer as u8;
    res |= (sync as u8) << 2;
    res |= (usage as u8) << 4;
    res
}

#[derive(Debug, Clone, Copy)]
pub struct UacDescriptorEndpoint {
    pub b_endpoint_address: u8,
    pub bm_attributes: u8,
    pub w_max_packet_size: u16,
    pub b_interval: u8,
    pub b_refresh: u8,
    pub b_synch_address: u8,
}

impl UacDescriptorEndpoint {
    pub fn deserialize(mut buffer: &mut &[u8]) -> UacDescriptorEndpoint {
        let format = structure!("<BBHBBB");
        let (b_endpoint_address, bm_attributes, w_max_packet_size, b_interval, b_refresh, b_synch_address) = format.unpack_from(&mut buffer).unwrap();
        let msg = UacDescriptorEndpoint { b_endpoint_address, bm_attributes, w_max_packet_size, b_interval, b_refresh, b_synch_address };
        return msg;
    }
    pub fn serialize(&self, mut buffer: impl Write) {
        let format = structure!("<BBBBHBBB");
        format.pack_into(&mut buffer,
                         self.size() as u8, UsbDescriptorTypes::Endpoint as u8,
                         self.b_endpoint_address, self.bm_attributes, self.w_max_packet_size, self.b_interval, self.b_refresh, self.b_synch_address
        ).unwrap();
    }
    pub fn size(&self) -> usize {
        structure!("<BBBBHBBB").size()
    }
    pub fn is_in(&self) -> bool {
        &self.b_endpoint_address & LIBUSB_ENDPOINT_DIR_MASK == LIBUSB_ENDPOINT_IN
    }
    pub fn is_out(&self) -> bool {
        &self.b_endpoint_address & LIBUSB_ENDPOINT_DIR_MASK == LIBUSB_ENDPOINT_OUT
    }
}

#[derive(Debug, Clone, Copy)]
pub struct DescriptorEndpoint {
    pub b_endpoint_address: u8,
    pub bm_attributes: u8,
    pub w_max_packet_size: u16,
    pub b_interval: u8,
}

impl DescriptorEndpoint {
    pub fn ep(&self) -> u8 {
        &self.b_endpoint_address & LIBUSB_ENDPOINT_ADDRESS_MASK
    }
    pub fn is_in(&self) -> bool {
        &self.b_endpoint_address & LIBUSB_ENDPOINT_DIR_MASK == LIBUSB_ENDPOINT_IN
    }
    pub fn is_out(&self) -> bool {
        &self.b_endpoint_address & LIBUSB_ENDPOINT_DIR_MASK == LIBUSB_ENDPOINT_OUT
    }
    pub fn transfer_type(&self) -> DescriptorTransferType {
        match &self.bm_attributes & LIBUSB_TRANSFER_TYPE_MASK {
            LIBUSB_TRANSFER_TYPE_CONTROL => DescriptorTransferType::Control,
            LIBUSB_TRANSFER_TYPE_ISOCHRONOUS => DescriptorTransferType::Isochronous,
            LIBUSB_TRANSFER_TYPE_BULK => DescriptorTransferType::Bulk,
            LIBUSB_TRANSFER_TYPE_INTERRUPT => DescriptorTransferType::Interrupt,
            LIBUSB_TRANSFER_TYPE_BULK_STREAM => DescriptorTransferType::BulkStream,
            _ => DescriptorTransferType::Invalid,
        }
    }
    pub fn synch_type(&self) -> Result<SynchType, Error> {
        if self.is_iso_transfer() {
            match &self.bm_attributes & LIBUSB_ISO_SYNC_TYPE_MASK {
                LIBUSB_ISO_SYNC_TYPE_NONE => Ok(SynchType::None),
                LIBUSB_ISO_SYNC_TYPE_ASYNC => Ok(SynchType::Asynchronous),
                LIBUSB_ISO_SYNC_TYPE_ADAPTIVE => Ok(SynchType::Adaptive),
                LIBUSB_ISO_SYNC_TYPE_SYNC => Ok(SynchType::Synchronous),
                _ => Ok(SynchType::Invalid),
            }
        } else {
            Err(anyhow!("DescriptorEndpoint is not isochronous"))
        }
    }
    pub fn usage_type(&self) -> Result<UsageType, Error> {
        if self.is_iso_transfer() {
            match &self.bm_attributes & LIBUSB_ISO_USAGE_TYPE_MASK {
                LIBUSB_ISO_USAGE_TYPE_DATA => Ok(UsageType::Data),
                LIBUSB_ISO_USAGE_TYPE_FEEDBACK => Ok(UsageType::Feedback),
                LIBUSB_ISO_USAGE_TYPE_IMPLICIT => Ok(UsageType::Implicit),
                _ => Ok(UsageType::Invalid),
            }
        } else {
            Err(anyhow!("DescriptorEndpoint is not isochronous"))
        }
    }
    pub fn is_speaker(&self) -> bool {
        self.is_iso_transfer() &&
        self.is_out() &&
        matches!(self.synch_type().unwrap(), SynchType::Asynchronous) &&
        matches!(self.usage_type().unwrap(), UsageType::Data)
    }
    pub fn is_mic(&self) -> bool {
        self.is_iso_transfer() &&
            self.is_in() &&
            matches!(self.synch_type().unwrap(), SynchType::Asynchronous) &&
            matches!(self.usage_type().unwrap(), UsageType::Data)
    }
    pub fn is_control_transfer(&self) -> bool {
        match self.transfer_type() { DescriptorTransferType::Control => true, _ => false }
    }
    pub fn is_iso_transfer(&self) -> bool {
        match self.transfer_type() { DescriptorTransferType::Isochronous => true, _ => false }
    }
    pub fn is_bulk_transfer(&self) -> bool {
        match self.transfer_type() { DescriptorTransferType::Bulk => true, _ => false }
    }
    pub fn is_interrupt_transfer(&self) -> bool {
        match self.transfer_type() { DescriptorTransferType::Interrupt => true, _ => false }
    }
    pub fn is_bulk_stream_transfer(&self) -> bool {
        match self.transfer_type() { DescriptorTransferType::BulkStream => true, _ => false }
    }
    pub fn transfer_type_is_valid(&self) -> bool {
        match self.transfer_type() { DescriptorTransferType::Invalid => false, _ => true }
    }
    pub fn serialize(&self, mut buffer: impl Write) {
        let format = structure!("<BBBBHB");
        format.pack_into(&mut buffer,
                         self.size() as u8, UsbDescriptorTypes::Endpoint as u8,
                         self.b_endpoint_address, self.bm_attributes, self.w_max_packet_size, self.b_interval
        ).unwrap();
    }
    pub fn deserialize(mut buffer: &mut &[u8]) -> DescriptorEndpoint {
        let format = structure!("<BBHB");
        let (b_endpoint_address, bm_attributes, w_max_packet_size, b_interval) = format.unpack_from(&mut buffer).unwrap();
        DescriptorEndpoint { b_endpoint_address, bm_attributes, w_max_packet_size, b_interval }
    }
    pub fn size(&self) -> usize {
        structure!("<BBBBHB").size()
    }
}

#[derive(Debug, Clone)]
pub enum DescriptorTypes {
    Root(),
    Unknown(DescriptorUnknown),
    Device(DescriptorDevice),
    CsDevice(DescriptorCsDevice),
    Config(DescriptorConfig),
    Interface(DescriptorInterface),
    CsInterface(DescriptorCsInterface),
    InterfaceAssociation(UsbInterfaceAssocDescriptor),
    Endpoint(DescriptorEndpoint),
    UacEndpoint(UacDescriptorEndpoint),
    HidEndpoint(DescriptorEndpoint),
    SsEpComp(UsbSsEpCompDescriptor),
    SspIsochEpComp(UsbSspIsochEpCompDescriptor),
    CsEndpoint(DescriptorCsEndpoint),
    UvcInputHeader(DescriptorUvcInputHeader),
    UacAcHeader(Uac1AcHeaderDescriptor),
    UacInputTerminal(UacInputTerminalDescriptor),
    UacOutputTerminal(Uac1OutputTerminalDescriptor),
    UacFeatureUnit(UacFeatureUnitDescriptor),
    UacAsGeneral(Uac1AsHeaderDescriptor),
    UacFormatTypeI(UacFormatTypeIContinuousDescriptor),
    UacFormatTypeUnknown(DescriptorUacFormatTypeUnknown),
    UacIsoEndpointDescriptor(UacIsoEndpointDescriptor),
    DescriptorUvcFormatUncompressed(DescriptorUvcFormatUncompressed),
    DescriptorUvcFormatMjpeg(DescriptorUvcFormatMjpeg),
    DescriptorUvcFrameUncompressed(DescriptorUvcFrameUncompressed),
    DescriptorUvcFrameMjpeg(DescriptorUvcFrameMjpeg),
    UvcFormatFrameBased(DescriptorUvcFormatFrameBased),
    UvcFrameFrameBased(DescriptorUvcFrameFrameBased),
    DescriptorUvcVsInterfaceUnknown(DescriptorUvcVsInterfaceUnknown),
    DescriptorUvcVcInterfaceUnknown(DescriptorUvcVcInterfaceUnknown),
    DescriptorUacInterfaceUnknown(DescriptorUacInterfaceUnknown),
    UvcHeaderDescriptor(UvcHeaderDescriptor),
    UvcVcInputTerminal(UvcInputTerminalDescriptor),
    UvcVcProcessingUnit(UvcProcessingUnitDescriptor),
    UvcVcExtensionUnit(UvcExtensionUnitDescriptor),
    UvcVcOutputTerminal(UvcOutputTerminalDescriptor),
}

impl DescriptorTypes {
    pub fn is_uvc_format(&self) -> bool {
        match self {
            DescriptorTypes::DescriptorUvcFormatMjpeg(_) => true,
            DescriptorTypes::DescriptorUvcFormatUncompressed(_) => true,
            DescriptorTypes::UvcFormatFrameBased(_) => true,
            _ => false,
        }
    }

    pub fn is_fmt_child(&self) -> bool {
        match self {
            DescriptorTypes::DescriptorUvcFrameMjpeg(_) => true,
            DescriptorTypes::DescriptorUvcFrameUncompressed(_) => true,
            DescriptorTypes::UvcFrameFrameBased(_) => true,
            DescriptorTypes::DescriptorUvcVsInterfaceUnknown(_) => true,
            _ => false,
        }
    }
}
