use self::{
    etable::ETable,
    imtable::{IMTable, ValueType},
    mtable::{memory_event_of_step, MTable},
};
use crate::{AsContext, Global, Memory};
use std::vec::Vec;
use wasmi_core::UntypedVal;

pub mod etable;
pub mod imtable;
pub mod mtable;

#[derive(Debug)]
pub struct Tracer {
    pub imtable: IMTable,
    pub etable: ETable,
}

impl Default for Tracer {
    fn default() -> Self {
        Self::new()
    }
}

impl Tracer {
    pub fn new() -> Self {
        Tracer {
            imtable: IMTable::default(),
            etable: ETable::default(),
        }
    }

    pub fn push_init_memory(&mut self, mem_ref: Memory, context: impl AsContext) {
        let pages: u32 = mem_ref.ty(&context).initial_pages().into();
        for i in 0..(pages * 8192) {
            let mut buf = [0u8; 8];
            mem_ref
                .read(&context, (i * 8).try_into().unwrap(), &mut buf)
                .unwrap();
            self.imtable
                .push(false, true, i, i, ValueType::I64, u64::from_le_bytes(buf));
        }

        let max_pages = mem_ref.ty(&context).maximum_pages();
        self.imtable.push(
            false,
            true,
            pages * 8192,
            max_pages
                .map(|limit| u32::from(limit) * 8192 - 1)
                .unwrap_or(u32::MAX),
            ValueType::I64,
            0,
        );
    }

    pub(crate) fn push_global(
        &mut self,
        global_idx: u32,
        global_ref: &Global,
        context: impl AsContext,
    ) {
        let vtype = global_ref.ty(&context);
        let vtype_content = global_ref.ty(&context).content();
        let val = UntypedVal::from(global_ref.get(&context));
        self.imtable.push(
            true,
            vtype.mutability().is_mut(),
            global_idx,
            global_idx,
            vtype_content.into(),
            val.to_bits(),
        )
    }

    pub fn get_mtable(&self) -> MTable {
        let mentries = self
            .etable
            .entries()
            .iter()
            .map(|entry| memory_event_of_step(entry, &mut 1))
            .collect::<Vec<Vec<_>>>()
            .concat();

        MTable::new(mentries)
    }
}
