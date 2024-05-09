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

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum AccessType {
    Read,
    Write,
    Init,
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

#[derive(Debug, Default)]
pub struct MTable(Vec<MemoryTableEntry>);

impl MTable {
    pub fn new(mentries: Vec<MemoryTableEntry>) -> MTable {
        MTable(mentries)
    }

    pub fn entries(&self) -> &Vec<MemoryTableEntry> {
        &self.0
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
