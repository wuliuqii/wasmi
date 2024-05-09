use std::vec::Vec;
use wasmi_core::ValType;

use super::mtable::LocationType;

#[derive(Debug)]
pub enum ValueType {
    I64,
    I32,
    F32,
    F64,
    FuncRef,
    ExternRef,
}

impl From<ValType> for ValueType {
    fn from(v: ValType) -> Self {
        match v {
            ValType::I32 => Self::I32,
            ValType::I64 => Self::I64,
            ValType::F32 => Self::F32,
            ValType::F64 => Self::F64,
            ValType::FuncRef => Self::FuncRef,
            ValType::ExternRef => Self::ExternRef,
        }
    }
}

#[derive(Debug)]
pub struct IMTableEntry {
    pub ltype: LocationType,
    pub is_mutable: bool,
    pub start_offset: u32,
    pub end_offset: u32,
    pub vtype: ValueType,
    pub value: u64,
}

#[derive(Debug, Default)]
pub struct IMTable(Vec<IMTableEntry>);

impl IMTable {
    pub(crate) fn push(
        &mut self,
        is_global: bool,
        is_mutable: bool,
        start_offset: u32,
        end_offset: u32,
        vtype: ValueType,
        value: u64,
    ) {
        self.0.push(IMTableEntry {
            ltype: if is_global {
                LocationType::Global
            } else {
                LocationType::Heap
            },
            is_mutable,
            start_offset,
            end_offset,
            vtype,
            value,
        });
    }
}
