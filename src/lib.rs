#![allow(dead_code)] // TODO: tests around all code

#[macro_use]
extern crate log;
#[macro_use]
extern crate anyhow;
#[macro_use]
extern crate num_derive;
#[macro_use]
extern crate structure;

mod uac_proto;
mod usb_proto;
mod uvc_proto;
mod logger;

use std::fmt;
use std::io::{Read, Write};

use anyhow::Error;
use libusb1_sys::constants::{LIBUSB_CLASS_AUDIO, LIBUSB_CLASS_HID, LIBUSB_CLASS_VIDEO};
use num_traits::FromPrimitive;
use structure::byteorder::{ReadBytesExt, WriteBytesExt};
use uuid::Uuid;

use crate::uac_proto::{DescriptorUacFormatTypeUnknown, DescriptorUacInterfaceUnknown, Uac1AcHeaderDescriptor, Uac1AsHeaderDescriptor, Uac1OutputTerminalDescriptor, UacDescriptorSubtypes, UacFeatureUnitDescriptor, UacFormatTypeI, UacFormatTypeIContinuousDescriptor, UacInputTerminalDescriptor, UacInterfaceSubclass, UacInterfaceSubtypes, UacIsoEndpointDescriptor};
use crate::usb_proto::{DescriptorConfig, DescriptorCsDevice, DescriptorCsEndpoint, DescriptorCsInterface, DescriptorEndpoint, DescriptorInterface, DescriptorTypes, DescriptorUnknown, IfaceAltSetting, UacDescriptorEndpoint, UsbDescriptorHeader, UsbDescriptorTypes, UsbInterfaceAssocDescriptor, UsbSsEpCompDescriptor, UsbSspIsochEpCompDescriptor};
use crate::uvc_proto::{DescriptorUvcFormatFrameBased, DescriptorUvcFormatMjpeg, DescriptorUvcFormatUncompressed, DescriptorUvcFrameFrameBased, DescriptorUvcFrameMjpeg, DescriptorUvcFrameUncompressed, DescriptorUvcInputHeader, DescriptorUvcVcInterfaceUnknown, DescriptorUvcVsInterfaceUnknown, MockVideoConfig, MockVideoFormat, UncompressedFormats, UvcExtensionUnitDescriptor, UvcHeaderDescriptor, UvcInputTerminalDescriptor, UvcInterfaceSubClass, UvcOutputTerminalDescriptor, UvcProcessingUnitDescriptor, UvcVcDescriptorSubtypes, UvcVsDescriptorSubtypes};

#[derive(Debug, Clone)]
pub struct TreeNode {
    pub children: Vec<TreeNode>,
    pub parsed: DescriptorTypes,
}

pub struct Size2d {
    pub width: u32,
    pub height: u32,
}

impl Size2d {
    pub fn new(width: u32, height: u32) -> Size2d {
        Size2d { width, height }
    }
}

impl TreeNode {
    pub fn new() -> TreeNode {
        return TreeNode { children: vec![], parsed: DescriptorTypes::Root() };
    }

    pub fn shallow_clone(&self) -> TreeNode {
        let mut new_node = self.clone();
        new_node.children.truncate(0);
        new_node
    }

    pub fn has_audio(&self) -> bool {
        if let DescriptorTypes::Interface(iface) = &self.parsed {
            if iface.is_audio() { return true; };
        };
        for child in &self.children {
            let res = child.has_audio();
            if res { return true; }
        }
        return false;
    }

    pub fn has_video(&self) -> bool {
        if let DescriptorTypes::Interface(iface) = &self.parsed {
            if iface.is_video_streaming() { return true; };
        };
        for child in &self.children {
            let res = child.has_video();
            if res { return true; }
        }
        return false;
    }

    pub fn replace_node(&mut self, new_node: &TreeNode, cb: fn(parsed: &DescriptorTypes) -> bool) {
        for i in 0..self.children.len() {
            if cb(&self.children.get_mut(i).unwrap().parsed) {
                self.children.remove(i);
                self.children.insert(i, new_node.clone());
            }
            self.children.get_mut(i).unwrap().replace_node(new_node, cb);
        }
    }

    pub fn get_node(&self, cb: fn(parsed: &DescriptorTypes) -> bool) -> Option<&TreeNode> {
        if cb(&self.parsed) {
            return Some(self);
        }
        for child in &self.children {
            let res = child.get_node(cb);
            if res.is_some() { return res; }
        }
        return None;
    }

    pub fn get_ep(&self) -> Option<&TreeNode> {
        self.get_node(|parsed| match parsed {
            DescriptorTypes::Endpoint(_) => true,
            _ => false
        })
    }

    pub fn get_uac_ep(&self) -> Option<&TreeNode> {
        self.get_node(|parsed| match parsed {
            DescriptorTypes::UacEndpoint(_) => true,
            _ => false
        })
    }

    pub fn get_ss_ep_comp(&self) -> Option<&TreeNode> {
        if let DescriptorTypes::SsEpComp(_) = &self.parsed { return Some(&self); };
        for child in &self.children {
            let res = child.get_ss_ep_comp();
            if res.is_some() { return res; }
        }
        return None;
    }

    pub fn get_uac_fmt(&self) -> Option<&TreeNode> {
        self.get_node(|parsed| match parsed {
            DescriptorTypes::UacFormatTypeI(_) => true,
            _ => false
        })
    }

    pub fn get_iface_by_num(&self, iface_setting: IfaceAltSetting) -> Option<&TreeNode> {
        match &self.parsed {
            DescriptorTypes::Interface(me) => {
                if me.b_interface_number == iface_setting.iface && me.b_alternate_setting == iface_setting.alt {
                    return Some(&self);
                }
            }
            _ => {}
        }
        for child in &self.children {
            let res = child.get_iface_by_num(iface_setting);
            if res.is_some() { return res; }
        }
        return None;
    }

    pub fn get_iface_by_ep(&self, ep: u8) -> Option<&TreeNode> {
        match &self.parsed {
            DescriptorTypes::Endpoint(me) => {
                if me.b_endpoint_address == ep {
                    return Some(&self);
                }
            }
            _ => {}
        }
        for child in &self.children {
            let res = child.get_iface_by_ep(ep);
            if let Some(res) = res {
                if let DescriptorTypes::Interface(_) = &res.parsed {
                    return Some(res); // If it's an interface, we're done
                } else {
                    return Some(self); // Otherwise go up the tree till we find an interface
                }
            }
        }
        return None;
    }

    // TODO: verify no cameras have multiple input headers...
    pub fn get_uvc_input_hdr(&mut self) -> Option<&mut TreeNode> {
        match &self.parsed {
            DescriptorTypes::UvcInputHeader(_) => return Some(self),
            _ => {}
        }
        for child in &mut self.children {
            let res = child.get_uvc_input_hdr();
            if res.is_some() { return res; }
        }
        return None;
    }

    pub fn get_format_by_idx(&self, idx: u8) -> Option<&TreeNode> {
        match &self.parsed {
            DescriptorTypes::DescriptorUvcFormatMjpeg(me) => if me.b_format_index == idx { return Some(&self); },
            DescriptorTypes::DescriptorUvcFormatUncompressed(me) => if me.b_format_index == idx { return Some(&self); },
            _ => {}
        }
        for child in &self.children {
            let res = child.get_format_by_idx(idx);
            if res.is_some() { return res; }
        }
        return None;
    }

    pub fn get_frame_by_idx(&self, idx: u8) -> Option<&TreeNode> {
        match &self.parsed {
            DescriptorTypes::DescriptorUvcFrameMjpeg(me) => if me.b_frame_index == idx { return Some(&self); },
            DescriptorTypes::DescriptorUvcFrameUncompressed(me) => if me.b_frame_index == idx { return Some(&self); },
            _ => {}
        }
        for child in &self.children {
            let res = child.get_frame_by_idx(idx);
            if res.is_some() { return res; }
        }
        return None;
    }

    pub fn get_video_cfg(&self, fmt_idx: u8, frame_idx: u8, fps: i32) -> Result<MockVideoConfig, Error> {
        let fmt_node = self.get_format_by_idx(fmt_idx).ok_or(anyhow!("UVC format not found!"))?;
        let fmt = match fmt_node.parsed {
            DescriptorTypes::DescriptorUvcFormatMjpeg(_) => MockVideoFormat::Mjpeg,
            DescriptorTypes::DescriptorUvcFormatUncompressed(f) => {
                match f.guid_format {
                    UncompressedFormats::YUY2 => MockVideoFormat::Yuy2,
                    UncompressedFormats::NV12 => MockVideoFormat::Nv12,
                    _ => return Err(anyhow!("Invalid format: {}", f.guid_format))
                }
            }
            _ => return Err(anyhow!("Invalid format: {:?}", fmt_node.parsed))
        };

        let frame = fmt_node.get_frame_by_idx(frame_idx).ok_or(anyhow!("UVC frame type not found!"))?;
        let sz = frame.frame_sz()?;
        let fmt_info = MockVideoConfig::new(sz.width, sz.height, fps, fmt);
        Ok(fmt_info)
    }

    pub fn frame_sz(&self) -> Result<Size2d, Error> {
        let sz = match &self.parsed {
            DescriptorTypes::DescriptorUvcFrameMjpeg(frame) => Size2d::new(frame.w_width as u32, frame.w_height as u32),
            DescriptorTypes::DescriptorUvcFrameUncompressed(frame) => Size2d::new(frame.w_width as u32, frame.w_height as u32),
            _ => Err(anyhow!("Attempt to get frame size from unsupported type: {:?}", self.parsed))?
        };
        Ok(sz)
    }

    pub fn fix_tree(&mut self) {
        let mut tmp_buf = vec![];
        self.serialize(&mut tmp_buf).unwrap();
        let iface_cnt = self.find_ifaces().len();

        match &mut self.parsed {
            DescriptorTypes::Config(conf) => {
                conf.w_total_length = tmp_buf.len() as u16;
                conf.b_num_interfaces = iface_cnt as u8;
            }
            DescriptorTypes::UvcInputHeader(ref mut hdr) => {
                hdr.w_total_length = tmp_buf.len() as u16;
            }
            DescriptorTypes::DescriptorUvcFormatMjpeg(ref mut fmt) => {
                fmt.b_num_frame_descriptors = self.children.iter().filter(|child| match child.parsed {
                    DescriptorTypes::DescriptorUvcFrameMjpeg(_) => true,
                    _ => false
                }).count() as u8;
                // fix b_default_frame_index < first_frame_idx
                for child in self.children.iter() {
                    if let DescriptorTypes::DescriptorUvcFrameMjpeg(ref frame) = child.parsed {
                        if fmt.b_default_frame_index < frame.b_frame_index {
                            fmt.b_default_frame_index = frame.b_frame_index;
                        }
                        break;
                    }
                }
                // fix b_default_frame_index > last_frame_idx
                for child in self.children.iter().rev() {
                    if let DescriptorTypes::DescriptorUvcFrameMjpeg(ref frame) = child.parsed {
                        if fmt.b_default_frame_index > frame.b_frame_index {
                            fmt.b_default_frame_index = frame.b_frame_index;
                        }
                        break;
                    }
                }
            }
            DescriptorTypes::DescriptorUvcFormatUncompressed(ref mut fmt) => {
                fmt.b_num_frame_descriptors = self.children.iter().filter(|child| match child.parsed {
                    DescriptorTypes::DescriptorUvcFrameUncompressed(_) => true,
                    _ => false
                }).count() as u8;
                // fix b_default_frame_index < first_frame_idx
                for child in self.children.iter() {
                    if let DescriptorTypes::DescriptorUvcFrameUncompressed(ref frame) = child.parsed {
                        if fmt.b_default_frame_index < frame.b_frame_index {
                            fmt.b_default_frame_index = frame.b_frame_index;
                        }
                        break;
                    }
                }
                // fix b_default_frame_index > last_frame_idx
                for child in self.children.iter().rev() {
                    if let DescriptorTypes::DescriptorUvcFrameUncompressed(ref frame) = child.parsed {
                        if fmt.b_default_frame_index > frame.b_frame_index {
                            fmt.b_default_frame_index = frame.b_frame_index;
                        }
                        break;
                    }
                }
            }
            DescriptorTypes::UvcFormatFrameBased(ref mut fmt) => {
                fmt.b_num_frame_descriptors = self.children.iter().filter(|child| match child.parsed {
                    DescriptorTypes::UvcFrameFrameBased(_) => true,
                    _ => false
                }).count() as u8;
                // fix b_default_frame_index < first_frame_idx
                for child in self.children.iter() {
                    if let DescriptorTypes::UvcFrameFrameBased(ref frame) = child.parsed {
                        if fmt.b_default_frame_index < frame.b_frame_index {
                            fmt.b_default_frame_index = frame.b_frame_index;
                        }
                        break;
                    }
                }
                // fix b_default_frame_index > last_frame_idx
                for child in self.children.iter().rev() {
                    if let DescriptorTypes::UvcFrameFrameBased(ref frame) = child.parsed {
                        if fmt.b_default_frame_index > frame.b_frame_index {
                            fmt.b_default_frame_index = frame.b_frame_index;
                        }
                        break;
                    }
                }
            }
            _ => (),
        }

        self.children.iter_mut().for_each(|child| {
            child.fix_tree();
        });
    }

    pub fn find_mic_iface(&self, mut iface: Option<u8>) -> Option<u8> {
        match self.parsed {
            DescriptorTypes::Interface(i) => {
                iface.replace(i.b_interface_number);
            }
            DescriptorTypes::UacEndpoint(ep) => {
                if ep.is_in() {
                    return iface;
                }
            }
            _ => {}
        }
        for child in self.children.iter() {
            let ret = child.find_mic_iface(iface);
            if ret.is_some() {
                return ret;
            }
        }
        None
    }

    pub fn find_mic_ep(&self) -> Option<u8> {
        match self.parsed {
            DescriptorTypes::UacEndpoint(ep) => {
                if ep.is_in() {
                    return Some(ep.b_endpoint_address);
                }
            }
            _ => {}
        }
        for child in self.children.iter() {
            let ret = child.find_mic_ep();
            if ret.is_some() {
                return ret;
            }
        }
        None
    }

    pub fn find_spkr_ep(&self) -> Option<u8> {
        match self.parsed {
            DescriptorTypes::UacEndpoint(ep) => {
                if ep.is_out() {
                    return Some(ep.b_endpoint_address);
                }
            }
            _ => {}
        }
        for child in self.children.iter() {
            let ret = child.find_spkr_ep();
            if ret.is_some() {
                return ret;
            }
        }
        None
    }

    pub fn find_spkr_iface(&self, mut iface: Option<u8>) -> Option<u8> {
        match self.parsed {
            DescriptorTypes::Interface(i) => {
                iface.replace(i.b_interface_number);
            }
            DescriptorTypes::UacEndpoint(ep) => {
                if ep.is_out() {
                    return iface;
                }
            }
            _ => {}
        }
        for child in self.children.iter() {
            let ret = child.find_spkr_iface(iface);
            if ret.is_some() {
                return ret;
            }
        }
        None
    }

    pub fn find_hid_ep(&self) -> Option<u8> {
        match self.parsed {
            DescriptorTypes::HidEndpoint(ep) => {
                if ep.is_out() {
                    return Some(ep.b_endpoint_address);
                }
            }
            _ => {}
        }
        for child in self.children.iter() {
            let ret = child.find_hid_ep();
            if ret.is_some() {
                return ret;
            }
        }
        None
    }

    pub fn find_hid_iface(&self, mut iface: Option<u8>) -> Option<u8> {
        match self.parsed {
            DescriptorTypes::Interface(i) => {
                iface.replace(i.b_interface_number);
                if i.b_interface_class == LIBUSB_CLASS_HID {
                    return iface;
                }
            }
            _ => {}
        }
        for child in self.children.iter() {
            let ret = child.find_hid_iface(iface);
            if ret.is_some() {
                return ret;
            }
        }
        None
    }

    pub fn find_uac_ifaces(&self) -> Vec<u8> {
        let mut ids = match self.parsed {
            DescriptorTypes::Interface(iface) => {
                if iface.is_audio() && iface.b_alternate_setting == 0 {
                    vec![iface.b_interface_number]
                } else {
                    vec![]
                }
            }
            _ => vec![]
        };
        for child in self.children.iter() {
            let mut child_ids = child.find_uac_ifaces();
            ids.append(&mut child_ids);
        }
        ids
    }

    pub fn find_non_uac_ifaces(&self) -> Vec<u8> {
        let mut ids = match self.parsed {
            DescriptorTypes::Interface(iface) => {
                if !iface.is_audio() && iface.b_alternate_setting == 0 {
                    vec![iface.b_interface_number]
                } else {
                    vec![]
                }
            }
            _ => vec![]
        };
        for child in self.children.iter() {
            let mut child_ids = child.find_non_uac_ifaces();
            ids.append(&mut child_ids);
        }
        ids
    }

    pub fn find_uvc_ifaces(&self) -> Vec<u8> {
        let mut ids = match self.parsed {
            DescriptorTypes::Interface(iface) => {
                if iface.is_video_streaming() && iface.b_alternate_setting == 0 {
                    vec![iface.b_interface_number]
                } else {
                    vec![]
                }
            }
            _ => vec![]
        };
        for child in self.children.iter() {
            let mut child_ids = child.find_uvc_ifaces();
            ids.append(&mut child_ids);
        }
        ids
    }

    pub fn find_non_uvc_ifaces(&self) -> Vec<u8> {
        let mut ids = match self.parsed {
            DescriptorTypes::Interface(iface) => {
                if !iface.is_video_streaming() && iface.b_alternate_setting == 0 {
                    vec![iface.b_interface_number]
                } else {
                    vec![]
                }
            }
            _ => vec![]
        };
        for child in self.children.iter() {
            let mut child_ids = child.find_non_uvc_ifaces();
            ids.append(&mut child_ids);
        }
        ids
    }

    pub fn find_ifaces(&self) -> Vec<u8> {
        let mut ids = match self.parsed {
            DescriptorTypes::Interface(iface) => {
                if iface.b_alternate_setting == 0 {
                    vec![iface.b_interface_number]
                } else {
                    vec![]
                }
            }
            _ => vec![]
        };
        for child in self.children.iter() {
            let mut child_ids = child.find_ifaces();
            ids.append(&mut child_ids);
        }
        ids
    }

    pub fn num_uvc_formats(&self) -> usize {
        let num = match self.parsed {
            DescriptorTypes::DescriptorUvcFormatUncompressed(_) |
            DescriptorTypes::DescriptorUvcFormatMjpeg(_) |
            DescriptorTypes::UvcFormatFrameBased(_) => 1,
            _ => 0
        };

        num + self.children.iter().fold(0, |acc, child| {
            acc + child.num_uvc_formats()
        })
    }

    pub fn remove_high_fps(&mut self) {
        match &mut self.parsed {
            DescriptorTypes::UvcFrameFrameBased(frame) => {
                frame.dw_frame_interval.retain(|interval| *interval >= 333333);
            }
            DescriptorTypes::DescriptorUvcFrameMjpeg(frame) => {
                frame.dw_frame_interval.retain(|interval| *interval >= 333333);
            }
            DescriptorTypes::DescriptorUvcFrameUncompressed(frame) => {
                frame.dw_frame_interval.retain(|interval| *interval >= 333333);
            }
            _ => {} // ignore non frame things
        };
        self.children.iter_mut().for_each(|child| child.remove_high_fps());
    }

    pub fn remove_ifaces(&mut self, ids: &Vec<u8>) {
        self.children.retain(|child| match &child.parsed {
            DescriptorTypes::Interface(iface) => !ids.contains(&iface.b_interface_number),
            _ => true,
        });
        self.children.iter_mut().for_each(|child| child.remove_ifaces(ids));
    }

    pub fn remove_iface_assoc(&mut self, ids: &Vec<u8>) {
        self.children.retain(|child| match &child.parsed {
            DescriptorTypes::InterfaceAssociation(assoc) => {
                let mut retain = true;
                for id in ids.iter() {
                    if *id >= assoc.b_first_interface && *id <= assoc.last_iface() {
                        retain = false
                    }
                }
                retain
            }
            _ => true,
        });
        self.children.iter_mut().for_each(|child| child.remove_iface_assoc(ids))
    }

    pub fn remove_high_resolution(&mut self) /* -> Result<(), Error> */ {
        const MAX_PIXELS: u64 = 1280 * 720;
        // TODO: remove the entire format when all frames for a format are > 720p instead of
        // leaving them in

        // We track the min resolution of each format and check if none are <= MAX_PIXELS so that
        // each format always has at least one entry, if these higher resolutions are selected it
        // may cause an increased CPU usage it would be better to fully remove a format from
        // the descriptors but this has proved to do correctly
        let frame_min_pixel_count = self.children.iter().filter_map(|i| match &i.parsed {
            DescriptorTypes::UvcFrameFrameBased(frame) => Some(frame.w_width as u64 * frame.w_height as u64),
            _ => None
        }).min();
        let mjpeg_min_pixel_count = self.children.iter().filter_map(|i| match &i.parsed {
            DescriptorTypes::DescriptorUvcFrameMjpeg(frame) => Some(frame.w_width as u64 * frame.w_height as u64),
            _ => None
        }).min();

        let filter_uncompressed_by_pix_fmt = |i: &TreeNode, pix_fmt: Uuid| match &i.parsed {
            DescriptorTypes::DescriptorUvcFrameUncompressed(frame) => {
                match &self.parsed {
                    DescriptorTypes::DescriptorUvcFormatUncompressed(format) => {
                        if format.guid_format == pix_fmt {
                            Some(frame.w_width as u64 * frame.w_height as u64)
                        } else {
                            None
                        }
                    }
                    _ => {
                        warn!("FrameUncompressed with a parent that is not FormatUncompressed should not be possible");
                        None
                    }
                }
            }
            _ => None
        };
        let uncompressed_yuy2_min_pixel_count = self.children.iter().filter_map(|node| filter_uncompressed_by_pix_fmt(node, UncompressedFormats::YUY2)).min();
        let uncompressed_nv12_min_pixel_count = self.children.iter().filter_map(|node| filter_uncompressed_by_pix_fmt(node, UncompressedFormats::NV12)).min();

        if [frame_min_pixel_count, mjpeg_min_pixel_count, uncompressed_yuy2_min_pixel_count, uncompressed_nv12_min_pixel_count].iter().any(|min| min.map_or(false, |min| min > MAX_PIXELS))
        {
            warn!("A format contains only resolutions greater than 720p if this is used it may increase CPU usage");
        }

        let uncompressed_pix_fmt = match &self.parsed {
            DescriptorTypes::DescriptorUvcFormatUncompressed(format) => Some(format.guid_format),
            _ => None
        };

        self.children.retain(|child| match &child.parsed {
            DescriptorTypes::UvcFrameFrameBased(frame) => {
                let pixels = frame.w_width as u64 * frame.w_height as u64;
                let min_pixels = frame_min_pixel_count.unwrap_or(0);
                warn!("frame pixels {} min_pixels {}", pixels, min_pixels);
                pixels <= MAX_PIXELS || (min_pixels > MAX_PIXELS && pixels == min_pixels || min_pixels == 0)
            }
            DescriptorTypes::DescriptorUvcFrameMjpeg(frame) => {
                let pixels = frame.w_width as u64 * frame.w_height as u64;
                let min_pixels = mjpeg_min_pixel_count.unwrap_or(0);
                warn!("mjpeg pixels {} min_pixels {}", pixels, min_pixels);
                pixels <= MAX_PIXELS || (min_pixels > MAX_PIXELS && pixels == min_pixels || min_pixels == 0)
            }
            DescriptorTypes::DescriptorUvcFrameUncompressed(frame) => {
                let pixels = frame.w_width as u64 * frame.w_height as u64;
                let min_pixels = match uncompressed_pix_fmt {
                    Some(UncompressedFormats::YUY2) => {
                        warn!("uncompressed pixels {} min_pixels {} guid_format {:?}", pixels, uncompressed_yuy2_min_pixel_count.unwrap_or(0), UncompressedFormats::YUY2);
                        uncompressed_yuy2_min_pixel_count.unwrap_or(0)
                    }
                    Some(UncompressedFormats::NV12) => {
                        warn!("uncompressed pixels {} min_pixels {} guid_format {:?}", pixels, uncompressed_nv12_min_pixel_count.unwrap_or(0), UncompressedFormats::NV12);
                        uncompressed_nv12_min_pixel_count.unwrap_or(0)
                    }
                    Some(_) | None => {
                        // to be safe we keep any unknown children
                        warn!("Unknown uncompressed format {:?} frame {:?}", uncompressed_pix_fmt, frame);
                        0
                    }
                };
                pixels <= MAX_PIXELS || min_pixels > MAX_PIXELS || min_pixels == 0
            }
            _ => {
                true
            }
        });
        self.children.iter_mut().for_each(|child| child.remove_high_resolution())
    }

    pub fn is_audio_control(&self) -> Result<bool, Error> {
        match &self.parsed {
            DescriptorTypes::Interface(iface_desc) => {
                Ok(iface_desc.is_audio_control())
            }
            _ => { Err(anyhow!("Non-interface node")) }
        }
    }

    pub fn is_audio_streaming(&self) -> Result<bool, Error> {
        match &self.parsed {
            DescriptorTypes::Interface(iface_desc) => {
                Ok(iface_desc.is_audio_streaming())
            }
            _ => { Err(anyhow!("Non-interface node")) }
        }
    }

    pub fn is_video_streaming(&self, iface_setting: &IfaceAltSetting) -> Result<bool, Error> {
        if let Some(iface) = self.get_iface_by_num(*iface_setting) {
            match &iface.parsed {
                DescriptorTypes::Interface(iface_desc) => {
                    Ok(iface_desc.is_video_streaming())
                }
                _ => { Err(anyhow!("Non-interface node")) }
            }
        } else {
            Err(anyhow!("No interface associated with: {:?}", iface_setting))
        }
    }

    pub fn is_speaker_interface(&self) -> Result<bool, Error> {
        match &self.parsed {
            DescriptorTypes::Interface(_) => {
                let mut is_spkr_iface = false;
                self.children.iter().for_each(|child| is_spkr_iface = matches!(&child.parsed, DescriptorTypes::Endpoint(ep) if ep.is_speaker()));
                Ok(is_spkr_iface)
            }
            _ => { Err(anyhow!("Non-interface node")) }
        }
    }

    pub fn is_mic_interface(&self) -> Result<bool, Error> {
        match &self.parsed {
            DescriptorTypes::Interface(_) => {
                let mut is_spkr_iface = false;
                self.children.iter().for_each(|child| is_spkr_iface = matches!(&child.parsed, DescriptorTypes::Endpoint(ep) if ep.is_mic()));
                Ok(is_spkr_iface)
            }
            _ => { Err(anyhow!("Non-interface node")) }
        }
    }

    pub fn remove_h264(&mut self) -> Result<(), Error> {
        // Only operate on UvcInputHeader
        if let DescriptorTypes::UvcInputHeader(ref mut hdr) = &mut self.parsed {
            // Build new bma_controls
            let mut bma_ctrls = vec![];
            for (idx, node) in self.children.iter().enumerate() {
                match node.parsed {
                    DescriptorTypes::UvcFormatFrameBased(_) => {} // Skip adding BMA control
                    DescriptorTypes::DescriptorUvcFormatMjpeg(_) | DescriptorTypes::DescriptorUvcFormatUncompressed(_) => {
                        let start = idx * hdr.b_control_size as usize;
                        let end = start + hdr.b_control_size as usize;
                        let bma_ctrl = &hdr.bma_controls[start..end];
                        bma_ctrls.extend_from_slice(&bma_ctrl);
                    }
                    _ => Err(anyhow!("Unknown child type for UvcInputHeader: {:?}", node.parsed))?
                }
            }
            // Remove unwanted formats
            self.children.retain(|child| if let DescriptorTypes::UvcFormatFrameBased(_) = &child.parsed { false } else { true });
            // Set bma_controls to idxes
            hdr.bma_controls = bma_ctrls;
            Ok(())
        } else {
            Err(anyhow!("Can only remove formats from a UVC input header node"))
        }
    }

    pub fn serialize(&self, mut buffer: &mut Vec<u8>) -> Result<(), Error> {
        match &self.parsed {
            DescriptorTypes::Root() => (),
            DescriptorTypes::CsDevice(desc) => desc.serialize(&mut buffer),
            DescriptorTypes::Config(desc) => desc.serialize(&mut buffer),
            DescriptorTypes::Interface(desc) => desc.serialize(&mut buffer),
            DescriptorTypes::CsInterface(desc) => desc.serialize(&mut buffer),
            DescriptorTypes::InterfaceAssociation(desc) => desc.serialize(&mut buffer),
            DescriptorTypes::Endpoint(desc) => desc.serialize(&mut buffer),
            DescriptorTypes::UacEndpoint(desc) => desc.serialize(&mut buffer),
            DescriptorTypes::HidEndpoint(desc) => desc.serialize(&mut buffer),
            DescriptorTypes::CsEndpoint(desc) => desc.serialize(&mut buffer),
            DescriptorTypes::UvcInputHeader(desc) => desc.serialize(&mut buffer),
            DescriptorTypes::DescriptorUvcFormatUncompressed(desc) => desc.serialize(&mut buffer),
            DescriptorTypes::DescriptorUvcFormatMjpeg(desc) => desc.serialize(&mut buffer),
            DescriptorTypes::DescriptorUvcFrameUncompressed(desc) => desc.serialize(&mut buffer),
            DescriptorTypes::DescriptorUvcFrameMjpeg(desc) => desc.serialize(&mut buffer),
            DescriptorTypes::DescriptorUvcVsInterfaceUnknown(desc) => desc.serialize(&mut buffer),
            DescriptorTypes::DescriptorUvcVcInterfaceUnknown(desc) => desc.serialize(&mut buffer),
            DescriptorTypes::DescriptorUacInterfaceUnknown(desc) => desc.serialize(&mut buffer),
            DescriptorTypes::UacAcHeader(desc) => desc.serialize(&mut buffer),
            DescriptorTypes::UacInputTerminal(desc) => desc.serialize(&mut buffer),
            DescriptorTypes::UacOutputTerminal(desc) => desc.serialize(&mut buffer),
            DescriptorTypes::UacFeatureUnit(desc) => desc.serialize(&mut buffer).unwrap(),
            DescriptorTypes::UacAsGeneral(desc) => desc.serialize(&mut buffer),
            DescriptorTypes::UacIsoEndpointDescriptor(desc) => desc.serialize(&mut buffer),
            DescriptorTypes::UacFormatTypeI(desc) => desc.serialize(&mut buffer)?,
            DescriptorTypes::UacFormatTypeUnknown(desc) => desc.serialize(&mut buffer),
            DescriptorTypes::UvcFormatFrameBased(desc) => desc.serialize(&mut buffer),
            DescriptorTypes::UvcFrameFrameBased(desc) => desc.serialize(&mut buffer),
            DescriptorTypes::UvcHeaderDescriptor(desc) => desc.serialize(&mut buffer),
            DescriptorTypes::UvcVcInputTerminal(desc) => desc.serialize(&mut buffer),
            DescriptorTypes::UvcVcProcessingUnit(desc) => desc.serialize(&mut buffer),
            DescriptorTypes::UvcVcExtensionUnit(desc) => desc.serialize(&mut buffer),
            DescriptorTypes::UvcVcOutputTerminal(desc) => desc.serialize(&mut buffer),
            DescriptorTypes::SsEpComp(desc) => desc.serialize(&mut buffer),
            DescriptorTypes::SspIsochEpComp(desc) => desc.serialize(&mut buffer),
            DescriptorTypes::Unknown(desc) => {
                buffer.write_u8(desc.bytes.len() as u8 + 2u8).unwrap();
                buffer.write_u8(desc.desc_type).unwrap();
                buffer.write_all(&desc.bytes).unwrap();
            }
            _ => panic!("Cannot serialize unknown type: {:?}", self.parsed)
        }
        for child in self.children.iter() {
            child.serialize(&mut buffer).unwrap();
        }
        Ok(())
    }

    pub fn deserialize(slice: &mut &[u8]) -> Result<TreeNode, Error> {
        let root = parse_list(slice);
        let root = pivot_cfg_desc(&root);

        // https://www.beyondlogic.org/usbnutshell/usb5.shtml#InterfaceDescriptors
        let mut new_idx = 0;
        let root = pivot_iface_children(&root, None, &mut new_idx);

        let mut new_idx = 0;
        let root = pivot_alt_settings(&root, None, &mut new_idx);

        let mut new_idx = 0;
        let root = pivot_uvc_input_hdr(&root, None, &mut new_idx, usize::MAX);

        let mut new_idx = 0;
        let root = pivot_iface_assoc(&root, None, &mut new_idx);

        let mut new_root = TreeNode::new();
        pivot_uvc_fmt_hdr(&root, &mut new_root, 0);

        Ok(new_root)
    }
}

trait RecursiveDisplay {
    fn recursive_fmt(&self, f: &mut fmt::Formatter<'_>, depth: u32) -> fmt::Result;
}

impl RecursiveDisplay for TreeNode {
    fn recursive_fmt(&self, f: &mut fmt::Formatter<'_>, depth: u32) -> fmt::Result {
        // handy for debugging
        // let mut tmp_buf = vec![];
        // serialize_tree(&mut tmp_buf, &self).unwrap();
        // write!(f, "{} bytes {}{:?}\n", tmp_buf.len(), (0..depth).map(|_| "\t").collect::<String>(), self.parsed)?;

        write!(f, "{}{:?}\n", (0..depth).map(|_| "\t").collect::<String>(), self.parsed)?;
        for child in self.children.iter() {
            child.recursive_fmt(f, depth + 1)?;
        }
        return Ok(());
    }
}

impl fmt::Display for TreeNode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.recursive_fmt(f, 0)
    }
}

fn uvc_iface_factory(buffer: &mut &[u8], subclass: &mut u8, len: u8) -> DescriptorTypes {
    match FromPrimitive::from_u8(*subclass) {
        Some(UvcInterfaceSubClass::VideoStreaming) => {
            let iface_subclass = buffer.read_u8().unwrap();
            match FromPrimitive::from_u8(iface_subclass) {
                Some(UvcVsDescriptorSubtypes::InputHeader) => DescriptorTypes::UvcInputHeader(DescriptorUvcInputHeader::deserialize(buffer)),
                Some(UvcVsDescriptorSubtypes::FormatUncompressed) => DescriptorTypes::DescriptorUvcFormatUncompressed(DescriptorUvcFormatUncompressed::deserialize(buffer)),
                Some(UvcVsDescriptorSubtypes::FormatMjpeg) => DescriptorTypes::DescriptorUvcFormatMjpeg(DescriptorUvcFormatMjpeg::deserialize(buffer)),
                Some(UvcVsDescriptorSubtypes::FrameUncompressed) => DescriptorTypes::DescriptorUvcFrameUncompressed(DescriptorUvcFrameUncompressed::deserialize(buffer)),
                Some(UvcVsDescriptorSubtypes::FrameMjpeg) => DescriptorTypes::DescriptorUvcFrameMjpeg(DescriptorUvcFrameMjpeg::deserialize(buffer)),
                Some(UvcVsDescriptorSubtypes::FormatFrameBased) => DescriptorTypes::UvcFormatFrameBased(DescriptorUvcFormatFrameBased::deserialize(buffer)),
                Some(UvcVsDescriptorSubtypes::FrameFrameBased) => DescriptorTypes::UvcFrameFrameBased(DescriptorUvcFrameFrameBased::deserialize(buffer)),
                _ => {
                    let mut bytes = vec![0u8; buffer.len()];
                    buffer.read_exact(&mut bytes).unwrap();
                    DescriptorTypes::DescriptorUvcVsInterfaceUnknown(DescriptorUvcVsInterfaceUnknown { iface_subclass, bytes })
                }
            }
        }
        Some(UvcInterfaceSubClass::VideoControl) => {
            let iface_subclass = buffer.read_u8().unwrap();
            match FromPrimitive::from_u8(iface_subclass) {
                Some(UvcVcDescriptorSubtypes::UvcVcHeader) => DescriptorTypes::UvcHeaderDescriptor(UvcHeaderDescriptor::deserialize(buffer)),
                Some(UvcVcDescriptorSubtypes::UvcVcInputTerminal) => DescriptorTypes::UvcVcInputTerminal(UvcInputTerminalDescriptor::deserialize(buffer, len)),
                Some(UvcVcDescriptorSubtypes::UvcVcProcessingUnit) => DescriptorTypes::UvcVcProcessingUnit(UvcProcessingUnitDescriptor::deserialize(buffer, len)),
                Some(UvcVcDescriptorSubtypes::UvcVcExtensionUnit) => DescriptorTypes::UvcVcExtensionUnit(UvcExtensionUnitDescriptor::deserialize(buffer)),
                Some(UvcVcDescriptorSubtypes::UvcVcOutputTerminal) => DescriptorTypes::UvcVcOutputTerminal(UvcOutputTerminalDescriptor::deserialize(buffer)),
                _ => {
                    let mut bytes = vec![0u8; buffer.len()];
                    buffer.read_exact(&mut bytes).unwrap();
                    DescriptorTypes::DescriptorUvcVcInterfaceUnknown(DescriptorUvcVcInterfaceUnknown { iface_subclass, bytes })
                }
            }
        }
        _ => {
            warn!("Unknown uvc interface: subclass={:#04x}", subclass);
            let mut desc = vec![0u8; buffer.len()];
            buffer.read_exact(&mut desc).unwrap();
            return DescriptorTypes::CsInterface(DescriptorCsInterface { bytes: desc });
        }
    }
}

fn uac_fmt_factory(buffer: &mut &[u8]) -> DescriptorTypes {
    let format_type = buffer.read_u8().unwrap();
    match FromPrimitive::from_u8(format_type) {
        Some(UacFormatTypeI::Pcm) => DescriptorTypes::UacFormatTypeI(UacFormatTypeIContinuousDescriptor::deserialize(buffer).unwrap()),
        _ => {
            warn!("Unknown uac format: type={:#04x}", format_type);
            let mut desc = vec![0u8; buffer.len()];
            buffer.read_exact(&mut desc).unwrap();
            return DescriptorTypes::UacFormatTypeUnknown(DescriptorUacFormatTypeUnknown { format_type, bytes: desc });
        }
    }
}

fn uac_ep_factory(buffer: &mut &[u8], subclass: &mut u8) -> DescriptorTypes {
    match FromPrimitive::from_u8(*subclass) {
        // TODO: technically this is only an ISO EP if it's under a normal EP with type=iso - we need another tree pivoter to know for sure
        Some(UacInterfaceSubclass::AudioStreaming) => DescriptorTypes::UacIsoEndpointDescriptor(UacIsoEndpointDescriptor::deserialize(buffer).unwrap()),
        _ => {
            warn!("Unknown UAC endpoint: subclass={:#04x}", subclass);
            let mut desc = vec![0u8; buffer.len()];
            buffer.read_exact(&mut desc).unwrap();
            return DescriptorTypes::CsEndpoint(DescriptorCsEndpoint { bytes: desc });
        }
    }
}

fn uac_iface_factory(buffer: &mut &[u8], subclass: &mut u8) -> DescriptorTypes {
    match FromPrimitive::from_u8(*subclass) {
        Some(UacInterfaceSubclass::AudioControl) => {
            let iface_subclass = buffer.read_u8().unwrap();
            match FromPrimitive::from_u8(iface_subclass) {
                Some(UacDescriptorSubtypes::Header) => DescriptorTypes::UacAcHeader(Uac1AcHeaderDescriptor::deserialize(buffer)),
                Some(UacDescriptorSubtypes::InputTerminal) => DescriptorTypes::UacInputTerminal(UacInputTerminalDescriptor::deserialize(buffer)),
                Some(UacDescriptorSubtypes::FeatureUnit) => DescriptorTypes::UacFeatureUnit(UacFeatureUnitDescriptor::deserialize(buffer).unwrap()),
                Some(UacDescriptorSubtypes::OutputTerminal) => DescriptorTypes::UacOutputTerminal(Uac1OutputTerminalDescriptor::deserialize(buffer)),
                _ => {
                    let mut bytes = vec![0u8; buffer.len()];
                    buffer.read_exact(&mut bytes).unwrap();
                    DescriptorTypes::DescriptorUacInterfaceUnknown(DescriptorUacInterfaceUnknown { iface_subclass, bytes })
                }
            }
        }
        Some(UacInterfaceSubclass::AudioStreaming) => {
            let iface_subclass = buffer.read_u8().unwrap();
            match FromPrimitive::from_u8(iface_subclass) {
                Some(UacInterfaceSubtypes::General) => DescriptorTypes::UacAsGeneral(Uac1AsHeaderDescriptor::deserialize(buffer)),
                Some(UacInterfaceSubtypes::FormatType) => uac_fmt_factory(buffer),
                _ => {
                    let mut bytes = vec![0u8; buffer.len()];
                    buffer.read_exact(&mut bytes).unwrap();
                    DescriptorTypes::DescriptorUacInterfaceUnknown(DescriptorUacInterfaceUnknown { iface_subclass, bytes })
                }
            }
        }
        _ => {
            warn!("Unknown uac interface: subclass={:#04x}", subclass);
            let mut desc = vec![0u8; buffer.len()];
            buffer.read_exact(&mut desc).unwrap();
            return DescriptorTypes::CsInterface(DescriptorCsInterface { bytes: desc });
        }
    }
}

fn node_factory(desc_type: u8, mut buffer: &mut &[u8], class: &mut u8, subclass: &mut u8, len: u8) -> DescriptorTypes {
    match FromPrimitive::from_u8(desc_type) {
        Some(UsbDescriptorTypes::CsDevice) => {
            warn!("Unknown class specific device: class={:#04x} subclass={:#04x}", class, subclass);
            let mut desc = vec![0u8; buffer.len()];
            buffer.read_exact(&mut desc).unwrap();
            return DescriptorTypes::CsDevice(DescriptorCsDevice { bytes: desc });
        }
        Some(UsbDescriptorTypes::Config) => DescriptorTypes::Config(DescriptorConfig::deserialize(&mut buffer)),
        Some(UsbDescriptorTypes::InterfaceAssociation) => {
            let assc = UsbInterfaceAssocDescriptor::deserialize(&mut buffer);
            return DescriptorTypes::InterfaceAssociation(assc);
        }
        Some(UsbDescriptorTypes::Interface) => {
            let iface = DescriptorInterface::deserialize(&mut buffer);
            *class = iface.b_interface_class;
            *subclass = iface.b_interface_sub_class;
            return DescriptorTypes::Interface(iface);
        }
        Some(UsbDescriptorTypes::CsInterface) => {
            return match *class {
                LIBUSB_CLASS_VIDEO => uvc_iface_factory(buffer, subclass, len),
                LIBUSB_CLASS_AUDIO => uac_iface_factory(buffer, subclass),
                _ => {
                    warn!("Unknown class specific interface: class={:#04x} subclass={:#04x}", class, subclass);
                    let mut desc = vec![0u8; buffer.len()];
                    buffer.read_exact(&mut desc).unwrap();
                    DescriptorTypes::CsInterface(DescriptorCsInterface { bytes: desc })
                }
            };
        }
        Some(UsbDescriptorTypes::CsEndpoint) => {
            return match *class {
                LIBUSB_CLASS_AUDIO => uac_ep_factory(buffer, subclass),
                _ => {
                    warn!("Unknown class specific endpoint: class={:#04x} subclass={:#04x}", class, subclass);
                    let mut desc = vec![0u8; buffer.len()];
                    buffer.read_exact(&mut desc).unwrap();
                    return DescriptorTypes::CsEndpoint(DescriptorCsEndpoint { bytes: desc });
                }
            };
        }
        Some(UsbDescriptorTypes::Endpoint) => {
            if *class == LIBUSB_CLASS_AUDIO && len == 9 {
                DescriptorTypes::UacEndpoint(UacDescriptorEndpoint::deserialize(&mut buffer))
            } else {
                DescriptorTypes::Endpoint(DescriptorEndpoint::deserialize(&mut buffer))
            }
        }
        Some(UsbDescriptorTypes::SuperSpeedEpComp) => DescriptorTypes::SsEpComp(UsbSsEpCompDescriptor::deserialize(&mut buffer)),
        Some(UsbDescriptorTypes::SuperSpeedPlusIsochEpComp) =>
            DescriptorTypes::SspIsochEpComp(UsbSspIsochEpCompDescriptor::deserialize(&mut buffer)),
        _ => {
            warn!("Unknown descriptor type: {:#04x}", desc_type);
            let mut desc = vec![0u8; buffer.len()];
            buffer.read_exact(&mut desc).unwrap();
            return DescriptorTypes::Unknown(DescriptorUnknown { desc_type, bytes: desc });
        }
    }
}

pub fn parse_list(mut buffer: &mut &[u8]) -> TreeNode {
    let mut root = TreeNode {
        children: vec![],
        parsed: DescriptorTypes::Root(),
    };
    let mut class = 0u8;
    let mut subclass = 0u8;
    while buffer.len() > 0 {
        let hdr = UsbDescriptorHeader::deserialize(&mut buffer);
        let mut desc = vec![0u8; hdr.b_length as usize - 2];
        if buffer.len() < desc.len() {
            warn!("Could not read entire descriptor!");
            return root;
        }
        buffer.read_exact(&mut desc).unwrap();
        let mut slice = &desc[..];
        let node = node_factory(hdr.b_descriptor_type, &mut slice, &mut class, &mut subclass, hdr.b_length);
        if slice.len() > 0 {
            warn!("{} extra bytes after parsing node of type {}", slice.len(), hdr.b_descriptor_type);
        }
        root.children.push(TreeNode {
            children: vec![],
            parsed: node,
        });
    }
    info!("Read a root node with {} children", root.children.len());
    return root;
}

pub fn pivot_cfg_desc(root: &TreeNode) -> TreeNode {
    let mut new_root = TreeNode { children: vec![], parsed: DescriptorTypes::Root() };
    let mut cur_node: Option<usize> = None;
    let mut bytes_remaining = 0usize;
    for child in &root.children {
        if let DescriptorTypes::Config(conf) = &child.parsed {
            cur_node = Some(new_root.children.len());
            new_root.children.push(child.clone());
            bytes_remaining = conf.w_total_length as usize;
        } else if bytes_remaining > 0 {
            new_root.children[cur_node.unwrap()].children.push(child.clone());
            let mut tmp_buf = Vec::new();
            child.serialize(&mut tmp_buf).unwrap();
            bytes_remaining -= tmp_buf.len();
        }
    }
    return new_root;
}

pub fn pivot_uvc_input_hdr(node: &TreeNode, mut new_node: Option<TreeNode>, idx: &mut usize, mut bytes_remaining: usize) -> TreeNode {
    if new_node.is_none() {
        new_node.replace(TreeNode::new());
    }
    let mut new_node = new_node.unwrap();
    let len = node.children.len();
    while *idx < len && bytes_remaining > 0 {
        // Shallow clone each child
        let child = &node.children[*idx];
        let mut new_child = child.clone();
        new_child.children.truncate(0);

        // if we hit a UVC input header, recurse
        if let DescriptorTypes::UvcInputHeader(hdr) = &child.parsed {
            *idx += 1;
            let sz = hdr.w_total_length as usize - hdr.size();
            new_child = pivot_uvc_input_hdr(&node, Some(new_child), idx, sz);
            *idx -= 1;
        }

        let mut new_idx = 0;
        new_child = pivot_uvc_input_hdr(child, Some(new_child), &mut new_idx, usize::MAX);

        let mut tmp_buf = vec![];
        new_child.serialize(&mut tmp_buf).unwrap();
        bytes_remaining -= tmp_buf.len();

        new_node.children.push(new_child);
        *idx += 1;
    }
    return new_node;
}

pub fn pivot_iface_assoc(node: &TreeNode, mut new_node: Option<TreeNode>, idx: &mut usize) -> TreeNode {
    if new_node.is_none() {
        new_node.replace(TreeNode::new());
    }
    let mut new_node = new_node.unwrap();
    let len = node.children.len();
    while *idx < len {
        // Shallow clone each child
        let child = &node.children[*idx];
        let mut new_child = child.clone();
        new_child.children.truncate(0);

        if let DescriptorTypes::InterfaceAssociation(assoc) = &new_node.parsed {
            match &child.parsed {
                DescriptorTypes::Interface(iface) => {
                    if iface.b_interface_number < assoc.b_first_interface || iface.b_interface_number > assoc.last_iface() {
                        return new_node;
                    }
                }
                _ => return new_node,
            }
        } else {
            if let DescriptorTypes::InterfaceAssociation(_) = &child.parsed {
                *idx += 1;
                new_child = pivot_iface_assoc(&node, Some(new_child), idx);
                *idx -= 1;
            }
        }

        let mut new_idx = 0;
        new_child = pivot_iface_assoc(child, Some(new_child), &mut new_idx);

        new_node.children.push(new_child);
        *idx += 1;
    }
    return new_node;
}

pub fn pivot_uvc_fmt_hdr(node: &TreeNode, new_node: &mut TreeNode, mut idx: usize) -> usize {
    while idx < node.children.len() {
        let child = &node.children[idx];
        let mut new_child = child.shallow_clone();

        if new_node.parsed.is_uvc_format() {
            if !child.parsed.is_fmt_child() {
                return idx - 1;
            }
        } else {
            if child.parsed.is_uvc_format() {
                idx = pivot_uvc_fmt_hdr(&node, &mut new_child, idx + 1);
            }
        }
        pivot_uvc_fmt_hdr(child, &mut new_child, 0);

        new_node.children.push(new_child);
        idx += 1;
    }
    idx
}

pub fn pivot_iface_children(node: &TreeNode, mut new_node: Option<TreeNode>, idx: &mut usize) -> TreeNode {
    if new_node.is_none() {
        new_node.replace(TreeNode::new());
    }
    let mut new_node = new_node.unwrap();
    let len = node.children.len();
    while *idx < len {
        // Shallow clone each child
        let child = &node.children[*idx];
        let mut new_child = child.clone();
        new_child.children.truncate(0);

        if let DescriptorTypes::Interface(_) | DescriptorTypes::InterfaceAssociation(_) = &new_node.parsed {
            match &child.parsed {
                DescriptorTypes::Interface(_) => return new_node,
                DescriptorTypes::InterfaceAssociation(_) => return new_node,
                _ => (),
            }
        } else {
            if let DescriptorTypes::Interface(_) | DescriptorTypes::InterfaceAssociation(_) = &child.parsed {
                *idx += 1;
                new_child = pivot_iface_children(&node, Some(new_child), idx);
                *idx -= 1;
            }
        }

        let mut new_idx = 0;
        new_child = pivot_iface_children(child, Some(new_child), &mut new_idx);

        new_node.children.push(new_child);
        *idx += 1;
    }
    return new_node;
}

pub fn pivot_alt_settings(node: &TreeNode, mut new_node: Option<TreeNode>, idx: &mut usize) -> TreeNode {
    if new_node.is_none() {
        new_node.replace(TreeNode::new());
    }
    let mut new_node = new_node.unwrap();
    let len = node.children.len();
    while *idx < len {
        // Shallow clone each child
        let child = &node.children[*idx];
        let mut new_child = child.clone();
        new_child.children.truncate(0);

        let mut new_idx = 0;
        new_child = pivot_alt_settings(child, Some(new_child), &mut new_idx);

        if let DescriptorTypes::Interface(orig_iface) = &new_node.parsed {
            match &child.parsed {
                DescriptorTypes::Interface(cur_iface) => {
                    if cur_iface.b_interface_number != orig_iface.b_interface_number {
                        return new_node;
                    }
                }
                DescriptorTypes::InterfaceAssociation(_) => {
                    return new_node;
                }
                _ => (),
            }
        } else {
            if let DescriptorTypes::Interface(_) = &child.parsed {
                *idx += 1;
                new_child = pivot_alt_settings(&node, Some(new_child), idx);
                *idx -= 1;
            }
        }

        new_node.children.push(new_child);
        *idx += 1;
    }
    return new_node;
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::fs::File;
    use std::io::Read;

    use crate::logger::setup_logger;

    use super::*;

    fn setup() {
        setup_logger();
    }

    fn descriptor_test(text_filename: &str, bianry_filename: &str) {
        // setup
        setup();
        let txt_expected = read_txt_file(text_filename);
        let bin_expected = read_bin_file(bianry_filename);

        // exercise
        let mut slice = &bin_expected[..];
        let root = TreeNode::deserialize(&mut slice).expect("Could not deserialize");

        assert_txt(&txt_expected, &root);
        assert_bin(&bin_expected, &root);
    }

    fn assert_txt(txt_expected: &String, actual: &TreeNode) {
        let txt_actual = format!("{}", actual).replace("\r", "");
        let txt_expected = txt_expected.replace("\r", "");
        assert_eq!(txt_actual, txt_expected); // definitely correct order
    }

    fn assert_bin(bin_expected: &Vec<u8>, actual: &TreeNode) {
        let mut bin_actual = vec![];
        actual.serialize(&mut bin_actual).unwrap();
        assert_eq!(bin_actual, *bin_expected); // definitely correct order
    }

    #[test]
    fn test_meetup() {
        let txt_filename = "046d_0866_meetup_config_desc_0.txt";
        let bin_filename = "046d_0866_meetup_config_desc_0.bin";

        descriptor_test(txt_filename, &bin_filename);
    }

    #[test]
    fn test_jounivo() {
        let txt_filename = "0x4444_0x1234_20_JOUNIVO_JV601_config_desc_0.txt";
        let bin_filename = "0x4444_0x1234_20_JOUNIVO_JV601_config_desc_0.bin";

        descriptor_test(txt_filename, &bin_filename);
    }

    #[test]
    fn test_c925e() {
        let txt_filename = "0x046d_0x085b_7_Logitech_Webcam_C925e_config_desc_0.txt";
        let bin_filename = "0x046d_0x085b_7_Logitech_Webcam_C925e_config_desc_0.bin";

        descriptor_test(txt_filename, &bin_filename);
    }

    #[test]
    fn test_owl() {
        let txt_filename = "0x0204_0x2e43_33_Meeting_Owl_Pro_config_desc_0.txt";
        let bin_filename = "0x0204_0x2e43_33_Meeting_Owl_Pro_config_desc_0.bin";

        descriptor_test(txt_filename, &bin_filename);
    }

    #[test]
    fn test_c920() {
        let txt_filename = "0x0892_0x046d_31_HD_Pro_Webcam_C920_config_desc_0.txt";
        let bin_filename = "0x0892_0x046d_31_HD_Pro_Webcam_C920_config_desc_0.bin";

        descriptor_test(txt_filename, &bin_filename);
    }

    #[test]
    fn test_cs700_audio() {
        let txt_filename = "0x4030_0x0499_8_Yamaha_CS-700_config_desc_0.txt";
        let bin_filename = "0x4030_0x0499_8_Yamaha_CS-700_config_desc_0.bin";

        descriptor_test(txt_filename, &bin_filename);
    }

    #[test]
    fn test_cs700_video() {
        let txt_filename = "0x4032_0x0499_6_CS-700_Video_config_desc_0.txt";
        let bin_filename = "0x4032_0x0499_6_CS-700_Video_config_desc_0.bin";

        descriptor_test(txt_filename, &bin_filename);
    }

    #[test]
    fn test_meetup_usb3() {
        let txt_filename = "0x0866_0x046d_12_Logitech_MeetUp_config_desc_0.txt";
        let bin_filename = "0x0866_0x046d_12_Logitech_MeetUp_config_desc_0.bin";

        descriptor_test(txt_filename, &bin_filename);
    }

    #[test]
    fn test_vadio() {
        let txt_filename = "0x0018_0x25c1_HuddleSHOT_config_desc_0.txt";
        let bin_filename = "0x0018_0x25c1_HuddleSHOT_config_desc_0.bin";

        descriptor_test(txt_filename, &bin_filename);
    }

    #[test]
    fn test_generic_speaker() {
        let txt_filename = "0x2070_0x1908_7_USB2.0_Device_config_desc_0.txt";
        let bin_filename = "0x2070_0x1908_7_USB2.0_Device_config_desc_0.bin";

        descriptor_test(txt_filename, &bin_filename);
    }

    #[test]
    fn test_meetup_speakerphone() {
        descriptor_test("0x0867_0x046d_MeetUp_Speakerphone_config_desc_0.txt", "0x0867_0x046d_MeetUp_Speakerphone_config_desc_0.bin")
    }

    #[test]
    fn test_realtek_dac() {
        descriptor_test("0x48a8_0x0bda_realtek_dac_config_desc_0.txt", "0x48a8_0x0bda_realtek_dac_config_desc_0.bin");
    }

    #[test]
    fn test_razer() {
        // setup
        setup();
        let bin_expected = read_bin_file("13d3_56d5_razer_integrated_config_desc_0.bin");

        // exercise
        let mut slice = &bin_expected[..];
        let fmt_root = TreeNode::deserialize(&mut slice).expect("Could not deserialize");

        info!("tree:\n{}", fmt_root);
        let mut actual = vec![];
        fmt_root.serialize(&mut actual).unwrap();

        // assert
        assert_eq!(actual, bin_expected);
    }

    #[test]
    fn test_bose_vb1_camera() {
        let txt_file = "0xa213_0x05a7_114_Bose_Videobar_VB1_config_desc_0.txt";
        let bin_file = "0xa213_0x05a7_114_Bose_Videobar_VB1_config_desc_0.bin";
        descriptor_test(txt_file, bin_file);
    }

    #[test]
    fn test_meetup_remove_60hz() {
        // setup
        setup();
        let txt_expected = read_txt_file("046d_0866_meetup_config_desc_0_no_60hz.txt");
        let bin_expected = read_bin_file("046d_0866_meetup_config_desc_0.bin");

        // exercise
        let mut slice = &bin_expected[..];
        let mut actual = TreeNode::deserialize(&mut slice).expect("Could not deserialize");
        actual.remove_high_fps();
        actual.fix_tree();
        info!("low fps tree:\n{}", actual);

        // assert
        assert_txt(&txt_expected, &actual);
    }

    #[test]
    fn test_meetup_remove_uac() {
        // setup
        setup();
        let txt_expected = read_txt_file("0x0867_0x046d_MeetUp_Speakerphone_config_desc_0_no_uac.txt");
        let bin_expected = read_bin_file("0x0867_0x046d_MeetUp_Speakerphone_config_desc_0.bin");

        // exercise
        let mut slice = &bin_expected[..];
        let mut actual = TreeNode::deserialize(&mut slice).expect("Could not deserialize");
        let ids = actual.find_uac_ifaces();
        info!("ids=${:?}", ids);
        actual.remove_iface_assoc(&ids);
        actual.remove_ifaces(&ids);
        actual.fix_tree();
        info!("no uac tree:\n{}", actual);

        // assert
        assert_txt(&txt_expected, &actual);
    }

    #[test]
    fn test_vadio_remove_uac() {
        // setup
        setup();
        let txt_expected = read_txt_file("0x0018_0x25c1_HuddleSHOT_config_desc_0_no_uac.txt");
        let bin_expected = read_bin_file("0x0018_0x25c1_HuddleSHOT_config_desc_0.bin");

        // exercise
        let mut slice = &bin_expected[..];
        let mut actual = TreeNode::deserialize(&mut slice).expect("Could not deserialize");
        let ids = actual.find_uac_ifaces();
        info!("ids=${:?}", ids);
        actual.remove_iface_assoc(&ids);
        actual.remove_ifaces(&ids);
        actual.fix_tree();
        info!("no uac tree:\n{}", actual);

        // assert
        assert_txt(&txt_expected, &actual);
    }

    #[test]
    fn test_meetup_remove_1080p() {
        // setup
        setup();
        let txt_expected = read_txt_file("046d_0866_meetup_config_desc_0_720p.txt");
        let bin_orginal = read_bin_file("046d_0866_meetup_config_desc_0.bin");

        // exercise
        let mut slice = &bin_orginal[..];
        let mut actual = TreeNode::deserialize(&mut slice).expect("Could not deserialize");
        actual.remove_high_resolution();
        actual.fix_tree();
        info!("low resolution tree:\n{}", actual);

        // assert
        assert_txt(&txt_expected, &actual);
    }

    #[test]
    fn test_panacast_p50_high_resolution() {
        setup();
        let txt_expected = read_txt_file("0x0b0e_0x3013_Jabra_PanaCast_50_config_desc_0_high_res.txt");
        let bin_original = read_bin_file("0x0b0e_0x3013_Jabra_PanaCast_50_config_desc_0.bin");

        let mut slice = &bin_original[..];
        let mut actual = TreeNode::deserialize(&mut slice).expect("Could not deserialize");
        actual.remove_high_resolution();
        actual.fix_tree();
        info!("panacast p50 high res removed: \n{}", actual);

        assert_txt(&txt_expected, &actual);
    }

    #[test]
    fn test_bose_vb1_camera_remove_h264() {
        // setup
        setup();
        let txt_expected = read_txt_file("0xa213_0x05a7_114_Bose_Videobar_VB1_config_desc_0_no_h264.txt");
        let bin_input = read_bin_file("0xa213_0x05a7_114_Bose_Videobar_VB1_config_desc_0.bin");

        // exercise
        let mut slice = &bin_input[..];
        let mut actual = TreeNode::deserialize(&mut slice).expect("Could not deserialize");
        let hdr = actual.get_uvc_input_hdr().expect("Input header not found!");
        hdr.remove_h264().expect("Unable to remove h264 format!");
        actual.fix_tree();

        // Ensure tree matches
        info!("bose tree:\n{}", actual);
        assert_txt(&txt_expected, &actual);

        // Ensure we can re-parse without a panic
        let mut bin_actual = vec![];
        actual.serialize(&mut bin_actual).unwrap();
        let mut slice = &bin_actual[..];
        let _ = TreeNode::deserialize(&mut slice);
    }

    #[test]
    fn test_c920_remove_all() {
        // setup
        setup();
        let txt_expected = read_txt_file("0x0892_0x046d_31_HD_Pro_Webcam_C920_config_desc_0_removed.txt");
        let bin_input = read_bin_file("0x0892_0x046d_31_HD_Pro_Webcam_C920_config_desc_0.bin");

        // exercise
        let mut slice = &bin_input[..];
        let mut actual = TreeNode::deserialize(&mut slice).expect("Could not deserialize");
        let uac_iface_ids = actual.find_uac_ifaces();
        actual.remove_high_fps();
        actual.remove_high_resolution();
        actual.remove_iface_assoc(&uac_iface_ids);
        actual.remove_ifaces(&uac_iface_ids);
        actual.fix_tree();

        // Ensure tree matches
        info!("c920 tree:\n{}", actual);
        assert_txt(&txt_expected, &actual);

        // Ensure we can re-parse without a panic
        let mut bin_actual = vec![];
        actual.serialize(&mut bin_actual).unwrap();
        let mut slice = &bin_actual[..];
        let _ = TreeNode::deserialize(&mut slice);
    }

    fn read_txt_file(filename: &str) -> String {
        let filename = format!("test/fixtures/{}", filename);
        fs::read_to_string(&filename).expect("Something went wrong reading the file")
    }

    fn read_bin_file(filename: &str) -> Vec<u8> {
        let filename = format!("test/fixtures/{}", filename);
        let mut bin_file = File::open(&filename).expect("no file found");
        let metadata = fs::metadata(&filename).expect("unable to read metadata");
        let mut expected = vec![0; metadata.len() as usize];
        bin_file.read(&mut expected).expect("buffer overflow");
        expected
    }

    #[test]
    fn test_bose_vb1_speakerphone() {
        descriptor_test("0xa213_0x05a7_116_Bose_Videobar_VB1_config_desc_0.txt", "0xa213_0x05a7_116_Bose_Videobar_VB1_config_desc_0.bin");
    }

    #[test]
    fn test_poly_studio_x30() {
        let txt_filename = "0x9275_0x095d_7_Poly_Studio_X30_config_desc_0.txt";
        let bin_filename = "0x9275_0x095d_7_Poly_Studio_X30_config_desc_0.bin";

        descriptor_test(txt_filename, &bin_filename);
    }

    #[test]
    fn test_huddly_iq() {
        let txt_filename = "0x2bd9_0x0021_7_Huddly_IQ_config_desc_0.txt";
        let bin_filename = "0x2bd9_0x0021_7_Huddly_IQ_config_desc_0.bin";

        descriptor_test(txt_filename, &bin_filename);
    }
}
