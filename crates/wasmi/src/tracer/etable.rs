use crate::{engine::bytecode::Instruction, Val};
use core::fmt::{Display, Formatter};
use std::vec::Vec;

#[derive(Debug, Clone)]
pub(crate) struct IVal {
    pub val: Val,
    pub addr: usize,
}

impl Display for IVal {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        write!(f, "{:?} {:10}", self.val, self.addr)
    }
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

impl Display for BinOp {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        match self {
            BinOp::Add => write!(f, "add"),
            BinOp::Sub => write!(f, "sub"),
            BinOp::Mul => write!(f, "mul"),
            BinOp::Div => write!(f, "div"),
            BinOp::Min => write!(f, "min"),
            BinOp::Max => write!(f, "max"),
            BinOp::CopySign => write!(f, "copysign"),
            BinOp::UnsignedDiv => write!(f, "udiv"),
            BinOp::UnsignedRem => write!(f, "urem"),
            BinOp::SignedDiv => write!(f, "sdiv"),
            BinOp::SignedRem => write!(f, "srem"),
        }
    }
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

impl Display for StepInfo {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        match self {
            StepInfo::I32BinOp {
                class,
                left,
                right,
                result,
            } => {
                write!(f, "{:?} {:10} {:10} {:10} ", class, left, right, result)
            }
            StepInfo::Unimplemented(instr) => {
                write!(f, "unimplemented {:?}", instr)
            }
        }
    }
}

#[derive(Debug)]
pub struct ETableEntry {
    pub eid: u32,
    pub allocated_memory_pages: u32,
    pub step_info: StepInfo,
}

impl Display for ETableEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{:10} {:10} {}",
            self.eid, self.allocated_memory_pages, self.step_info
        )
    }
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

    pub fn show(&self) {
        println!(
            "{:10} {:10} {}",
            "eid", "allocated_memory_pages", "step_info"
        );

        for entry in self.entries() {
            println!("{}", entry);
        }
    }
}
