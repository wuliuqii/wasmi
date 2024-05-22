use core::fmt::Display;
use std::{println, vec, vec::Vec};

use crate::{
    etable::{ETableEntry, IVal, StepInfo},
    Val,
};

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LocationType {
    Stack,
    Heap,
    Global,
}

impl Display for LocationType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LocationType::Stack => write!(f, "Stack"),
            LocationType::Heap => write!(f, "Heap"),
            LocationType::Global => write!(f, "Global"),
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum AccessType {
    Read,
    Write,
    Init,
}

impl Display for AccessType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AccessType::Read => write!(f, "Read"),
            AccessType::Write => write!(f, "Write"),
            AccessType::Init => write!(f, "Init"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct MemoryTableEntry {
    pub eid: u32,
    pub emid: u32,
    pub addr: usize,
    pub ltype: LocationType,
    pub atype: AccessType,
    pub is_mutable: bool,
    pub value: Val,
}

impl Display for MemoryTableEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{:7} {:8} {:8} {:6} {:5} {:5} {:?}",
            self.eid, self.emid, self.addr, self.ltype, self.atype, self.is_mutable, self.value
        )
    }
}

#[derive(Debug, Default)]
pub struct MTable(Vec<MemoryTableEntry>);

impl MTable {
    pub fn new(mentries: Vec<MemoryTableEntry>) -> MTable {
        MTable(mentries)
    }

    pub fn entries(&self) -> &Vec<MemoryTableEntry> {
        &self.0
    }

    pub fn show(&self) {
        println!(
            "{:7} {:8} {:8} {:6} {:5} {:5} value",
            "eid", "emid", "addr", "ltype", "atype", "is_mutable",
        );

        for entry in self.entries() {
            println!("{}", entry);
        }
    }
}

pub fn memory_event_of_step(event: &ETableEntry, emid: &mut u32) -> Vec<MemoryTableEntry> {
    let eid = event.eid;

    match &event.step_info {
        StepInfo::I32BinOp {
            left,
            right,
            result,
            ..
        } => mem_op_from_stack_only_step(eid, emid, &[left, right], &[result]),
        StepInfo::Unimplemented(instr) => {
            println!("unimplemented {:?}", instr);
            vec![]
        }
    }
}

fn mem_op_from_stack_only_step(
    eid: u32,
    emid: &mut u32,
    read_value: &[&IVal],
    write_value: &[&IVal],
) -> Vec<MemoryTableEntry> {
    let mut mem_op = Vec::new();

    for ival in read_value {
        mem_op.push(MemoryTableEntry {
            eid,
            emid: *emid,
            addr: ival.addr,
            ltype: LocationType::Stack,
            atype: AccessType::Read,
            is_mutable: true,
            value: ival.val.clone(),
        });
        *emid = (*emid).checked_add(1).unwrap();
    }

    for ival in write_value {
        mem_op.push(MemoryTableEntry {
            eid,
            emid: *emid,
            addr: ival.addr,
            ltype: LocationType::Stack,
            atype: AccessType::Write,
            is_mutable: true,
            value: ival.val.clone(),
        });
        *emid = (*emid).checked_add(1).unwrap();
    }

    mem_op
}
