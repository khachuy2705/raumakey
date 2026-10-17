//! Minimal PE image writer for unit tests.
//!
//! Produces a one-section image with optional export, import, delay-load import, and resource
//! directories laid out the way MSVC-style linkers do. It exists so that inspection tests never
//! depend on real build artifacts.

use std::collections::BTreeMap;

const FILE_ALIGNMENT: u32 = 0x200;
const SECTION_ALIGNMENT: u32 = 0x1000;
const SECTION_RVA: u32 = 0x1000;
const HEADERS_SIZE: u32 = 0x200;
const NT_HEADERS_OFFSET: usize = 0x40;
const DATA_DIRECTORY_COUNT: usize = 16;

/// Description of a synthetic PE image.
#[derive(Clone, Debug)]
pub struct PeBuilder {
    /// COFF machine.
    pub machine: u16,
    /// PE32+ (64-bit) optional header.
    pub pe32_plus: bool,
    /// Sets `IMAGE_FILE_DLL`.
    pub dll: bool,
    /// Optional header subsystem.
    pub subsystem: u16,
    /// Named exports (all pointing at a code stub).
    pub exports: Vec<String>,
    /// Additional exports that have only an ordinal.
    pub ordinal_only_exports: usize,
    /// Imported DLL names.
    pub imports: Vec<String>,
    /// Delay-loaded DLL names.
    pub delay_imports: Vec<String>,
    /// Resources as `(type id, name id, data)`; each gets language 0x0409.
    pub resources: Vec<(u16, u16, Vec<u8>)>,
    /// Arbitrary bytes placed in the section (e.g. GUID constants or strings).
    pub payload: Vec<u8>,
}

impl PeBuilder {
    /// An image with no directories.
    pub fn new(machine: u16, pe32_plus: bool, dll: bool) -> Self {
        Self {
            machine,
            pe32_plus,
            dll,
            subsystem: 2,
            exports: Vec::new(),
            ordinal_only_exports: 0,
            imports: Vec::new(),
            delay_imports: Vec::new(),
            resources: Vec::new(),
            payload: Vec::new(),
        }
    }

    /// Serializes the image.
    pub fn build(&self) -> Vec<u8> {
        let mut section = vec![0xC3; 16]; // "code" stub at SECTION_RVA
        section.extend_from_slice(&self.payload);
        let mut directories = [(0u32, 0u32); DATA_DIRECTORY_COUNT];
        if !self.exports.is_empty() || self.ordinal_only_exports > 0 {
            directories[0] = self.write_exports(&mut section);
        }
        if !self.imports.is_empty() {
            directories[1] = self.write_imports(&mut section);
        }
        if !self.resources.is_empty() {
            directories[2] = self.write_resources(&mut section);
        }
        if !self.delay_imports.is_empty() {
            directories[13] = self.write_delay_imports(&mut section);
        }
        self.write_image(&section, &directories)
    }

    fn write_image(&self, section: &[u8], directories: &[(u32, u32)]) -> Vec<u8> {
        let fixed_optional = if self.pe32_plus { 112 } else { 96 };
        let optional_size = fixed_optional + 8 * DATA_DIRECTORY_COUNT;
        let raw_size = align_up(section.len() as u32, FILE_ALIGNMENT);
        let mut image = vec![0u8; HEADERS_SIZE as usize];
        image[..2].copy_from_slice(b"MZ");
        put_u32(&mut image, 0x3C, NT_HEADERS_OFFSET as u32);
        image[NT_HEADERS_OFFSET..NT_HEADERS_OFFSET + 4].copy_from_slice(b"PE\0\0");

        let file_header = NT_HEADERS_OFFSET + 4;
        let mut characteristics: u16 = 0x0002; // executable image
        characteristics |= if self.pe32_plus { 0x0020 } else { 0x0100 };
        if self.dll {
            characteristics |= 0x2000;
        }
        put_u16(&mut image, file_header, self.machine);
        put_u16(&mut image, file_header + 2, 1);
        put_u16(&mut image, file_header + 16, optional_size as u16);
        put_u16(&mut image, file_header + 18, characteristics);

        let optional = file_header + 20;
        put_u16(
            &mut image,
            optional,
            if self.pe32_plus { 0x20B } else { 0x10B },
        );
        put_u32(&mut image, optional + 16, SECTION_RVA); // entry point
        put_u32(&mut image, optional + 20, SECTION_RVA); // base of code
        if self.pe32_plus {
            image[optional + 24..optional + 32].copy_from_slice(&0x1_4000_0000u64.to_le_bytes());
        } else {
            put_u32(&mut image, optional + 28, 0x40_0000);
        }
        put_u32(&mut image, optional + 32, SECTION_ALIGNMENT);
        put_u32(&mut image, optional + 36, FILE_ALIGNMENT);
        put_u16(&mut image, optional + 40, 6);
        put_u16(&mut image, optional + 48, 6);
        put_u32(
            &mut image,
            optional + 56,
            SECTION_RVA + align_up(section.len() as u32, SECTION_ALIGNMENT),
        );
        put_u32(&mut image, optional + 60, HEADERS_SIZE);
        put_u16(&mut image, optional + 68, self.subsystem);
        put_u16(&mut image, optional + 70, 0x0160); // high entropy VA, dynamic base, NX
        let count_offset = optional + fixed_optional - 4;
        put_u32(&mut image, count_offset, DATA_DIRECTORY_COUNT as u32);
        for (index, (rva, size)) in directories.iter().enumerate() {
            let entry = optional + fixed_optional + 8 * index;
            put_u32(&mut image, entry, *rva);
            put_u32(&mut image, entry + 4, *size);
        }

        let header = optional + optional_size;
        image[header..header + 8].copy_from_slice(b".rdata\0\0");
        put_u32(&mut image, header + 8, section.len() as u32);
        put_u32(&mut image, header + 12, SECTION_RVA);
        put_u32(&mut image, header + 16, raw_size);
        put_u32(&mut image, header + 20, HEADERS_SIZE);
        put_u32(&mut image, header + 36, 0x4000_0040);

        image.extend_from_slice(section);
        image.resize((HEADERS_SIZE + raw_size) as usize, 0);
        image
    }

    fn write_exports(&self, section: &mut Vec<u8>) -> (u32, u32) {
        align_vec(section, 4);
        let start = section.len();
        let names = self.exports.len();
        let functions = names + self.ordinal_only_exports;
        let functions_at = start + 40;
        let names_at = functions_at + 4 * functions;
        let ordinals_at = names_at + 4 * names;
        let dll_name_at = ordinals_at + 2 * names;
        section.resize(dll_name_at, 0);
        section.extend_from_slice(b"synthetic.dll\0");
        let mut name_offsets = Vec::new();
        for name in &self.exports {
            name_offsets.push(section.len());
            section.extend_from_slice(name.as_bytes());
            section.push(0);
        }

        put_u32(section, start + 12, rva(dll_name_at));
        put_u32(section, start + 16, 1); // ordinal base
        put_u32(section, start + 20, functions as u32);
        put_u32(section, start + 24, names as u32);
        put_u32(section, start + 28, rva(functions_at));
        put_u32(section, start + 32, rva(names_at));
        put_u32(section, start + 36, rva(ordinals_at));
        for index in 0..functions {
            put_u32(section, functions_at + 4 * index, SECTION_RVA);
        }
        for (index, offset) in name_offsets.iter().enumerate() {
            put_u32(section, names_at + 4 * index, rva(*offset));
            put_u16(section, ordinals_at + 2 * index, index as u16);
        }
        (rva(start), (section.len() - start) as u32)
    }

    fn write_imports(&self, section: &mut Vec<u8>) -> (u32, u32) {
        align_vec(section, 8);
        let start = section.len();
        let descriptors_len = 20 * (self.imports.len() + 1);
        section.resize(start + descriptors_len, 0);
        let thunk = if self.pe32_plus { 8 } else { 4 };
        for (index, dll) in self.imports.iter().enumerate() {
            let lookup = section.len();
            section.resize(lookup + 2 * thunk, 0); // empty lookup table and address table
            let address = lookup + thunk;
            let name = section.len();
            section.extend_from_slice(dll.as_bytes());
            section.push(0);
            align_vec(section, 8);
            let descriptor = start + 20 * index;
            put_u32(section, descriptor, rva(lookup));
            put_u32(section, descriptor + 12, rva(name));
            put_u32(section, descriptor + 16, rva(address));
        }
        (rva(start), descriptors_len as u32)
    }

    fn write_delay_imports(&self, section: &mut Vec<u8>) -> (u32, u32) {
        align_vec(section, 8);
        let start = section.len();
        let descriptors_len = 32 * (self.delay_imports.len() + 1);
        section.resize(start + descriptors_len, 0);
        for (index, dll) in self.delay_imports.iter().enumerate() {
            let slots = section.len();
            section.resize(slots + 24, 0); // module handle, address table, name table
            let name = section.len();
            section.extend_from_slice(dll.as_bytes());
            section.push(0);
            align_vec(section, 8);
            let descriptor = start + 32 * index;
            put_u32(section, descriptor, 1); // RVA-based
            put_u32(section, descriptor + 4, rva(name));
            put_u32(section, descriptor + 8, rva(slots));
            put_u32(section, descriptor + 12, rva(slots + 8));
            put_u32(section, descriptor + 16, rva(slots + 16));
        }
        (rva(start), descriptors_len as u32)
    }

    fn write_resources(&self, section: &mut Vec<u8>) -> (u32, u32) {
        align_vec(section, 8);
        let start = section.len();
        let mut tree: BTreeMap<u16, BTreeMap<u16, &[u8]>> = BTreeMap::new();
        for (kind, name, data) in &self.resources {
            tree.entry(*kind).or_default().insert(*name, data);
        }
        let leaves: usize = tree.values().map(BTreeMap::len).sum();
        let root_len = 16 + 8 * tree.len();
        let names_len: usize = tree.values().map(|names| 16 + 8 * names.len()).sum();
        let languages_len = leaves * (16 + 8);
        let entries_len = leaves * 16;
        section.resize(
            start + root_len + names_len + languages_len + entries_len,
            0,
        );

        let mut name_table = root_len;
        let mut language_table = root_len + names_len;
        let mut data_entry = root_len + names_len + languages_len;
        put_u16(section, start + 14, tree.len() as u16);
        for (type_index, (kind, names)) in tree.iter().enumerate() {
            let root_entry = start + 16 + 8 * type_index;
            put_u32(section, root_entry, u32::from(*kind));
            put_u32(section, root_entry + 4, 0x8000_0000 | name_table as u32);
            put_u16(section, start + name_table + 14, names.len() as u16);
            for (name_index, (name, data)) in names.iter().enumerate() {
                let name_entry = start + name_table + 16 + 8 * name_index;
                put_u32(section, name_entry, u32::from(*name));
                put_u32(section, name_entry + 4, 0x8000_0000 | language_table as u32);
                put_u16(section, start + language_table + 14, 1);
                put_u32(section, start + language_table + 16, 0x0409);
                put_u32(section, start + language_table + 20, data_entry as u32);
                align_vec(section, 4);
                let payload = section.len();
                section.extend_from_slice(data);
                put_u32(section, start + data_entry, rva(payload));
                put_u32(section, start + data_entry + 4, data.len() as u32);
                language_table += 16 + 8;
                data_entry += 16;
            }
            name_table += 16 + 8 * names.len();
        }
        (rva(start), (section.len() - start) as u32)
    }
}

fn rva(section_offset: usize) -> u32 {
    SECTION_RVA + section_offset as u32
}

fn align_up(value: u32, alignment: u32) -> u32 {
    value.div_ceil(alignment) * alignment
}

fn align_vec(bytes: &mut Vec<u8>, alignment: usize) {
    let len = bytes.len().div_ceil(alignment) * alignment;
    bytes.resize(len, 0);
}

fn put_u16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn put_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}
