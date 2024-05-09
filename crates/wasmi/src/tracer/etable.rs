use crate::{engine::bytecode::Instruction, Val};
use std::vec::Vec;

#[derive(Debug, Clone)]
pub(crate) struct IVal {
    pub val: Val,
    pub addr: usize,
}

#[derive(Copy, Clone, Debug)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Min,
    Max,
    CopySign,
    UnsignedDiv,
    UnsignedRem,
    SignedDiv,
    SignedRem,
}

#[derive(Debug, Clone)]
pub enum StepInfo {
    I32BinOp {
        class: BinOp,
        left: IVal,
        right: IVal,
        result: IVal,
    },
    Unimplemented(Instruction),
}

#[derive(Debug)]
pub struct ETableEntry {
    pub eid: u32,
    pub allocated_memory_pages: u32,
    pub step_info: StepInfo,
}

#[derive(Debug, Default)]
pub struct ETable(Vec<ETableEntry>);

impl ETable {
    pub fn entries(&self) -> &Vec<ETableEntry> {
        &self.0
    }

    pub fn push(&mut self, allocated_memory_pages: u32, step_info: StepInfo) {
        let entry = ETableEntry {
            eid: (self.entries().len() + 1).try_into().unwrap(),
            allocated_memory_pages,
            step_info,
        };

        self.0.push(entry);
    }
}
