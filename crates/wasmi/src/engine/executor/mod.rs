pub(crate) use self::stack::Stack;
use self::{
    instrs::{execute_instrs, execute_instrs_with_trace, CallKind, WasmOutcome},
    stack::CallFrame,
    trap::TaggedTrap,
};
use crate::{
    engine::{
        bytecode::{Register, RegisterSpan},
        cache::InstanceCache,
        code_map::InstructionPtr,
        CallParams,
        CallResults,
        EngineInner,
        EngineResources,
        FuncParams,
        ResumableCallBase,
        ResumableInvocation,
    },
    func::HostFuncEntity,
    AsContext,
    AsContextMut,
    Error,
    Func,
    FuncEntity,
    Instance,
    StoreContextMut,
    Tracer,
};

use std::{cell::RefCell, rc::Rc};

#[cfg(doc)]
use crate::{engine::StackLimits, Store};

mod instrs;
pub(crate) mod stack;
mod trap;

impl EngineInner {
    /// Executes the given [`Func`] with the given `params` and returns the `results`.
    ///
    /// Uses the [`StoreContextMut`] for context information about the Wasm [`Store`].
    ///
    /// # Errors
    ///
    /// If the Wasm execution traps or runs out of resources.
    pub fn execute_func<T, Results>(
        &self,
        ctx: StoreContextMut<T>,
        func: &Func,
        params: impl CallParams,
        results: Results,
    ) -> Result<<Results as CallResults>::Results, Error>
    where
        Results: CallResults,
    {
        let res = self.res.read();
        let mut stack = self.stacks.lock().reuse_or_new();
        let results = EngineExecutor::new(&res, &mut stack)
            .execute_root_func(ctx, func, params, results)
            .map_err(TaggedTrap::into_error);
        self.stacks.lock().recycle(stack);
        results
    }

    /// Executes the given [`Func`] with the given `params` and returns the `results`.
    ///
    /// Uses the [`StoreContextMut`] for context information about the Wasm [`Store`].
    ///
    /// # Errors
    ///
    /// If the Wasm execution traps or runs out of resources.
    pub fn execute_func_with_trace<T, Results>(
        &self,
        ctx: StoreContextMut<T>,
        func: &Func,
        params: impl CallParams,
        results: Results,
        tracer: Rc<RefCell<Tracer>>,
    ) -> Result<<Results as CallResults>::Results, Error>
    where
        Results: CallResults,
    {
        let res = self.res.read();
        let mut stack = self.stacks.lock().reuse_or_new();
        let results = EngineExecutor::new(&res, &mut stack)
            .execute_root_func_with_trace(ctx, func, params, results, tracer)
            .map_err(TaggedTrap::into_error);
        self.stacks.lock().recycle(stack);
        results
    }

    /// Executes the given [`Func`] resumably with the given `params` and returns the `results`.
    ///
    /// Uses the [`StoreContextMut`] for context information about the Wasm [`Store`].
    ///
    /// # Errors
    ///
    /// If the Wasm execution traps or runs out of resources.
    pub(crate) fn execute_func_resumable<T, Results>(
        &self,
        mut ctx: StoreContextMut<T>,
        func: &Func,
        params: impl CallParams,
        results: Results,
    ) -> Result<ResumableCallBase<<Results as CallResults>::Results>, Error>
    where
        Results: CallResults,
    {
        let res = self.res.read();
        let mut stack = self.stacks.lock().reuse_or_new();
        let results = EngineExecutor::new(&res, &mut stack).execute_root_func(
            ctx.as_context_mut(),
            func,
            params,
            results,
        );
        match results {
            Ok(results) => {
                self.stacks.lock().recycle(stack);
                Ok(ResumableCallBase::Finished(results))
            }
            Err(TaggedTrap::Wasm(error)) => {
                self.stacks.lock().recycle(stack);
                Err(error)
            }
            Err(TaggedTrap::Host {
                host_func,
                host_error,
                caller_results,
            }) => Ok(ResumableCallBase::Resumable(ResumableInvocation::new(
                ctx.as_context().store.engine().clone(),
                *func,
                host_func,
                host_error,
                caller_results,
                stack,
            ))),
        }
    }

    /// Resumes the given [`Func`] with the given `params` and returns the `results`.
    ///
    /// Uses the [`StoreContextMut`] for context information about the Wasm [`Store`].
    ///
    /// # Errors
    ///
    /// If the Wasm execution traps or runs out of resources.
    pub(crate) fn resume_func<T, Results>(
        &self,
        ctx: StoreContextMut<T>,
        mut invocation: ResumableInvocation,
        params: impl CallParams,
        results: Results,
    ) -> Result<ResumableCallBase<<Results as CallResults>::Results>, Error>
    where
        Results: CallResults,
    {
        let res = self.res.read();
        let host_func = invocation.host_func();
        let caller_results = invocation.caller_results();
        let results = EngineExecutor::new(&res, &mut invocation.stack).resume_func(
            ctx,
            host_func,
            params,
            caller_results,
            results,
        );
        match results {
            Ok(results) => {
                self.stacks.lock().recycle(invocation.take_stack());
                Ok(ResumableCallBase::Finished(results))
            }
            Err(TaggedTrap::Wasm(error)) => {
                self.stacks.lock().recycle(invocation.take_stack());
                Err(error)
            }
            Err(TaggedTrap::Host {
                host_func,
                host_error,
                caller_results,
            }) => {
                invocation.update(host_func, host_error, caller_results);
                Ok(ResumableCallBase::Resumable(invocation))
            }
        }
    }
}

/// The internal state of the Wasmi engine.
#[derive(Debug)]
pub struct EngineExecutor<'engine> {
    /// Shared and reusable generic engine resources.
    res: &'engine EngineResources,
    /// The value and call stacks.
    stack: &'engine mut Stack,
}

impl<'engine> EngineExecutor<'engine> {
    /// Creates a new [`EngineExecutor`] with the given [`StackLimits`].
    ///
    /// [`StackLimits`]: []
    pub fn new(res: &'engine EngineResources, stack: &'engine mut Stack) -> Self {
        Self { res, stack }
    }

    /// Executes the given [`Func`] using the given `params`.
    ///
    /// Stores the execution result into `results` upon a successful execution.
    ///
    /// # Errors
    ///
    /// - If the given `params` do not match the expected parameters of `func`.
    /// - If the given `results` do not match the the length of the expected results of `func`.
    /// - When encountering a Wasm or host trap during the execution of `func`.
    pub fn execute_root_func<T, Results>(
        &mut self,
        mut ctx: StoreContextMut<T>,
        func: &Func,
        params: impl CallParams,
        results: Results,
    ) -> Result<<Results as CallResults>::Results, TaggedTrap>
    where
        Results: CallResults,
    {
        self.stack.reset();
        match ctx.as_context().store.inner.resolve_func(func) {
            FuncEntity::Wasm(wasm_func) => {
                // We reserve space on the stack to write the results of the root function execution.
                let len_results = results.len_results();
                self.stack.values.reserve(len_results)?;
                // SAFETY: we just called reserve to fit all new values.
                unsafe { self.stack.values.extend_zeros(len_results) };
                let instance = *wasm_func.instance();
                let compiled_func = wasm_func.func_body();
                let ctx = ctx.as_context_mut();
                let compiled_func = self
                    .res
                    .code_map
                    .get(Some(ctx.store.inner.fuel_mut()), compiled_func)?;
                let (base_ptr, frame_ptr) = self.stack.values.alloc_call_frame(compiled_func)?;
                // Safety: We use the `base_ptr` that we just received upon allocating the new
                //         call frame which is guaranteed to be valid for this particular operation
                //         until deallocating the call frame again.
                //         Also we are providing call parameters which have been checked already to
                //         be exactly the length of the expected function arguments.
                unsafe { self.stack.values.fill_at(base_ptr, params.call_params()) };
                self.stack.calls.push(CallFrame::new(
                    InstructionPtr::new(compiled_func.instrs().as_ptr()),
                    frame_ptr,
                    base_ptr,
                    RegisterSpan::new(Register::from_i16(0)),
                    instance,
                ))?;
                self.execute_func(ctx)?;
            }
            FuncEntity::Host(host_func) => {
                // The host function signature is required for properly
                // adjusting, inspecting and manipulating the value stack.
                let (input_types, output_types) = self
                    .res
                    .func_types
                    .resolve_func_type(host_func.ty_dedup())
                    .params_results();
                // In case the host function returns more values than it takes
                // we are required to extend the value stack.
                let len_params = input_types.len();
                let len_results = output_types.len();
                let max_inout = len_params.max(len_results);
                self.stack.values.reserve(max_inout)?;
                // SAFETY: we just called reserve to fit all new values.
                unsafe { self.stack.values.extend_zeros(max_inout) };
                let values = &mut self.stack.values.as_slice_mut()[..len_params];
                for (value, param) in values.iter_mut().zip(params.call_params()) {
                    *value = param;
                }
                let host_func = *host_func;
                self.dispatch_host_func(ctx.as_context_mut(), host_func, HostFuncCaller::Root)?;
            }
        };
        let results = self.write_results_back(results);
        Ok(results)
    }

    /// Executes the given [`Func`] using the given `params`.
    ///
    /// Stores the execution result into `results` upon a successful execution.
    ///
    /// # Errors
    ///
    /// - If the given `params` do not match the expected parameters of `func`.
    /// - If the given `results` do not match the the length of the expected results of `func`.
    /// - When encountering a Wasm or host trap during the execution of `func`.
    pub fn execute_root_func_with_trace<T, Results>(
        &mut self,
        mut ctx: StoreContextMut<T>,
        func: &Func,
        params: impl CallParams,
        results: Results,
        tracer: Rc<RefCell<Tracer>>,
    ) -> Result<<Results as CallResults>::Results, TaggedTrap>
    where
        Results: CallResults,
    {
        self.stack.reset();
        match ctx.as_context().store.inner.resolve_func(func) {
            FuncEntity::Wasm(wasm_func) => {
                // We reserve space on the stack to write the results of the root function execution.
                let len_results = results.len_results();
                self.stack.values.reserve(len_results)?;
                // SAFETY: we just called reserve to fit all new values.
                unsafe { self.stack.values.extend_zeros(len_results) };
                let instance = *wasm_func.instance();
                let compiled_func = wasm_func.func_body();
                let ctx = ctx.as_context_mut();
                let compiled_func = self
                    .res
                    .code_map
                    .get(Some(ctx.store.inner.fuel_mut()), compiled_func)?;
                let (base_ptr, frame_ptr) = self.stack.values.alloc_call_frame(compiled_func)?;
                // Safety: We use the `base_ptr` that we just received upon allocating the new
                //         call frame which is guaranteed to be valid for this particular operation
                //         until deallocating the call frame again.
                //         Also we are providing call parameters which have been checked already to
                //         be exactly the length of the expected function arguments.
                unsafe { self.stack.values.fill_at(base_ptr, params.call_params()) };
                self.stack.calls.push(CallFrame::new(
                    InstructionPtr::new(compiled_func.instrs().as_ptr()),
                    frame_ptr,
                    base_ptr,
                    RegisterSpan::new(Register::from_i16(0)),
                    instance,
                ))?;
                self.execute_func_with_trace(ctx, tracer)?;
            }
            // TODO: implement host call trace
            FuncEntity::Host(host_func) => {
                // The host function signature is required for properly
                // adjusting, inspecting and manipulating the value stack.
                let (input_types, output_types) = self
                    .res
                    .func_types
                    .resolve_func_type(host_func.ty_dedup())
                    .params_results();
                // In case the host function returns more values than it takes
                // we are required to extend the value stack.
                let len_params = input_types.len();
                let len_results = output_types.len();
                let max_inout = len_params.max(len_results);
                self.stack.values.reserve(max_inout)?;
                // SAFETY: we just called reserve to fit all new values.
                unsafe { self.stack.values.extend_zeros(max_inout) };
                let values = &mut self.stack.values.as_slice_mut()[..len_params];
                for (value, param) in values.iter_mut().zip(params.call_params()) {
                    *value = param;
                }
                let host_func = *host_func;
                self.dispatch_host_func(ctx.as_context_mut(), host_func, HostFuncCaller::Root)?;
            }
        };
        let results = self.write_results_back(results);
        Ok(results)
    }

    /// Resumes the execution of the given [`Func`] using `params`.
    ///
    /// Stores the execution result into `results` upon a successful execution.
    ///
    /// # Errors
    ///
    /// - If the given `params` do not match the expected parameters of `func`.
    /// - If the given `results` do not match the the length of the expected results of `func`.
    /// - When encountering a Wasm or host trap during the execution of `func`.
    pub fn resume_func<T, Results>(
        &mut self,
        mut ctx: StoreContextMut<T>,
        _host_func: Func,
        params: impl CallParams,
        caller_results: RegisterSpan,
        results: Results,
    ) -> Result<<Results as CallResults>::Results, TaggedTrap>
    where
        Results: CallResults,
    {
        let caller = self
            .stack
            .calls
            .peek()
            .expect("must have caller call frame on stack upon function resumption");
        let mut caller_sp = unsafe { self.stack.values.stack_ptr_at(caller.base_offset()) };
        let call_params = params.call_params();
        let len_params = call_params.len();
        for (result, param) in caller_results.iter(len_params).zip(call_params) {
            unsafe { caller_sp.set(result, param) };
        }
        self.execute_func(ctx.as_context_mut())?;
        let results = self.write_results_back(results);
        Ok(results)
    }

    /// Executes the top most Wasm function on the [`Stack`] until the [`Stack`] is empty.
    ///
    /// # Errors
    ///
    /// When encountering a Wasm or host trap during execution.
    #[inline(never)]
    fn execute_func<T>(&mut self, mut ctx: StoreContextMut<T>) -> Result<(), TaggedTrap> {
        let mut cache = self
            .stack
            .calls
            .peek()
            .map(CallFrame::instance)
            .map(InstanceCache::from)
            .expect("must have frame on the call stack");
        loop {
            match self.execute_compiled_func(ctx.as_context_mut(), &mut cache)? {
                WasmOutcome::Return => {
                    // In this case the root function has returned.
                    // Therefore we can return from the entire execution.
                    return Ok(());
                }
                WasmOutcome::Call {
                    results,
                    ref host_func,
                    call_kind,
                } => {
                    let instance = *self
                        .stack
                        .calls
                        .peek()
                        .expect("caller must be on the stack")
                        .instance();
                    self.execute_host_func(&mut ctx, results, host_func, &instance, call_kind)?;
                }
            }
        }
    }

    /// Executes the top most Wasm function on the [`Stack`] until the [`Stack`] is empty.
    ///
    /// # Errors
    ///
    /// When encountering a Wasm or host trap during execution.
    #[inline(never)]
    fn execute_func_with_trace<T>(
        &mut self,
        mut ctx: StoreContextMut<T>,
        tracer: Rc<RefCell<Tracer>>,
    ) -> Result<(), TaggedTrap> {
        let mut cache = self
            .stack
            .calls
            .peek()
            .map(CallFrame::instance)
            .map(InstanceCache::from)
            .expect("must have frame on the call stack");
        loop {
            match self.execute_compiled_func_with_trace(
                ctx.as_context_mut(),
                &mut cache,
                tracer.clone(),
            )? {
                WasmOutcome::Return => {
                    // In this case the root function has returned.
                    // Therefore we can return from the entire execution.
                    return Ok(());
                }
                WasmOutcome::Call {
                    results,
                    ref host_func,
                    call_kind,
                } => {
                    let instance = *self
                        .stack
                        .calls
                        .peek()
                        .expect("caller must be on the stack")
                        .instance();
                    self.execute_host_func(&mut ctx, results, host_func, &instance, call_kind)?;
                }
            }
        }
    }

    fn execute_host_func<T>(
        &mut self,
        ctx: &mut StoreContextMut<'_, T>,
        results: RegisterSpan,
        func: &Func,
        instance: &Instance,
        call_kind: CallKind,
    ) -> Result<(), TaggedTrap> {
        let func_entity = match ctx.as_context().store.inner.resolve_func(func) {
            FuncEntity::Wasm(wasm_func) => {
                unreachable!("expected a host function but found: {wasm_func:?}")
            }
            FuncEntity::Host(host_func) => *host_func,
        };
        let result = self.dispatch_host_func(
            ctx.as_context_mut(),
            func_entity,
            HostFuncCaller::wasm(results, instance),
        );
        if matches!(call_kind, CallKind::Tail) {
            self.stack.calls.pop();
        }
        if self.stack.calls.peek().is_some() {
            // Case: There is a frame on the call stack.
            //
            // This is the default case and we can easily make host function
            // errors return a resumable call handle.
            result.map_err(|error| TaggedTrap::host(*func, error, results))?;
        } else {
            // Case: No frame is on the call stack. (edge case)
            //
            // This can happen if the host function was called by a tail call.
            // In this case we treat host function errors the same as if we called
            // the host function as root and do not allow to resume the call.
            result.map_err(TaggedTrap::Wasm)?;
        }
        Ok(())
    }
}

/// The caller of a host function call.
#[derive(Debug, Copy, Clone)]
enum HostFuncCaller<'a> {
    /// The host-side is itself the caller of the host function.
    Root,
    /// A compiled Wasm function is the caller of the host function.
    Wasm {
        /// The registers were the caller expects the results of the call.
        results: RegisterSpan,
        /// The instance to be used throughout the host function call.
        instance: &'a Instance,
    },
}

impl<'a> HostFuncCaller<'a> {
    /// Creates a [`HostFuncCaller::Wasm`].
    pub fn wasm(results: RegisterSpan, instance: &'a Instance) -> Self {
        Self::Wasm { results, instance }
    }

    /// Returns the [`RegisterSpan`] if `self` is a Wasm caller, otherwise returns `None`.
    pub fn results(&self) -> Option<RegisterSpan> {
        match *self {
            HostFuncCaller::Root => None,
            HostFuncCaller::Wasm { results, .. } => Some(results),
        }
    }

    /// Returns the [`Instance`] if `self` is a Wasm caller, otherwise returns `None`.
    pub fn instance(&self) -> Option<&Instance> {
        match self {
            HostFuncCaller::Root => None,
            HostFuncCaller::Wasm { instance, .. } => Some(instance),
        }
    }
}

impl<'engine> EngineExecutor<'engine> {
    /// Dispatches a host function call and returns its result.
    fn dispatch_host_func<T>(
        &mut self,
        ctx: StoreContextMut<T>,
        host_func: HostFuncEntity,
        caller: HostFuncCaller,
    ) -> Result<(), Error> {
        // The host function signature is required for properly
        // adjusting, inspecting and manipulating the value stack.
        let (input_types, output_types) = self
            .res
            .func_types
            .resolve_func_type(host_func.ty_dedup())
            .params_results();
        // In case the host function returns more values than it takes
        // we are required to extend the value stack.
        let len_inputs = input_types.len();
        let len_outputs = output_types.len();
        let max_inout = len_inputs.max(len_outputs);
        let values = self.stack.values.as_slice_mut();
        let params_results = FuncParams::new(
            values.split_at_mut(values.len() - max_inout).1,
            len_inputs,
            len_outputs,
        );
        // Now we are ready to perform the host function call.
        // Note: We need to clone the host function due to some borrowing issues.
        //       This should not be a big deal since host functions usually are cheap to clone.
        let trampoline = ctx
            .as_context()
            .store
            .resolve_trampoline(host_func.trampoline())
            .clone();
        trampoline
            .call(ctx, caller.instance(), params_results)
            .map_err(|error| {
                // Note: We drop the values that have been temporarily added to
                //       the stack to act as parameter and result buffer for the
                //       called host function. Since the host function failed we
                //       need to clean up the temporary buffer values here.
                //       This is required for resumable calls to work properly.
                self.stack.values.drop(max_inout);
                error
            })?;
        if let Some(results) = caller.results() {
            // Now the results need to be written back to where the caller expects them.
            let caller_offset = self
                .stack
                .calls
                .peek()
                .expect("caller must be on the stack")
                .base_offset();
            // # Safety (1)
            //
            // We can safely acquire the stack pointer to the caller's and callee's (host)
            // call frames because we just allocated the host call frame and can be sure that
            // they are different.
            // In the following we make sure to not access registers out of bounds of each
            // call frame since we rely on Wasm validation and proper Wasm translation to
            // provide us with valid result registers.
            let mut caller_sp = unsafe { self.stack.values.stack_ptr_at(caller_offset) };
            // # Safety: See Safety (1) above.
            let callee_sp = unsafe { self.stack.values.stack_ptr_last_n(max_inout) };
            let results = results.iter(len_outputs);
            let values = RegisterSpan::new(Register::from_i16(0)).iter(len_outputs);
            for (result, value) in results.zip(values) {
                // # Safety: See Safety (1) above.
                unsafe { caller_sp.set(result, callee_sp.get(value)) };
            }
            // Finally, the value stack needs to be truncated to its original size.
            self.stack.values.drop(max_inout);
        }
        Ok(())
    }

    /// Executes the given function `frame`.
    ///
    /// # Note
    ///
    /// This executes Wasm instructions until either the execution calls
    /// into a host function or the Wasm execution has come to an end.
    ///
    /// # Errors
    ///
    /// If the Wasm execution traps.
    #[inline(always)]
    fn execute_compiled_func<T>(
        &mut self,
        ctx: StoreContextMut<T>,
        cache: &mut InstanceCache,
    ) -> Result<WasmOutcome, Error> {
        let (store_inner, mut resource_limiter) = ctx.store.store_inner_and_resource_limiter_ref();
        let value_stack = &mut self.stack.values;
        let call_stack = &mut self.stack.calls;
        let code_map = &self.res.code_map;
        let func_types = &self.res.func_types;
        execute_instrs(
            store_inner,
            cache,
            value_stack,
            call_stack,
            code_map,
            func_types,
            &mut resource_limiter,
        )
    }

    /// Executes the given function `frame`.
    ///
    /// # Note
    ///
    /// This executes Wasm instructions until either the execution calls
    /// into a host function or the Wasm execution has come to an end.
    ///
    /// # Errors
    ///
    /// If the Wasm execution traps.
    #[inline(always)]
    fn execute_compiled_func_with_trace<T>(
        &mut self,
        ctx: StoreContextMut<T>,
        cache: &mut InstanceCache,
        tracer: Rc<RefCell<Tracer>>,
    ) -> Result<WasmOutcome, Error> {
        let (store_inner, mut resource_limiter) = ctx.store.store_inner_and_resource_limiter_ref();
        let value_stack = &mut self.stack.values;
        let call_stack = &mut self.stack.calls;
        let code_map = &self.res.code_map;
        let func_types = &self.res.func_types;
        execute_instrs_with_trace(
            store_inner,
            cache,
            value_stack,
            call_stack,
            code_map,
            func_types,
            &mut resource_limiter,
            tracer,
        )
    }

    /// Writes the results of the function execution back into the `results` buffer.
    ///
    /// # Note
    ///
    /// The value stack is empty after this operation.
    ///
    /// # Panics
    ///
    /// - If the `results` buffer length does not match the remaining amount of stack values.
    #[inline]
    fn write_results_back<Results>(&mut self, results: Results) -> <Results as CallResults>::Results
    where
        Results: CallResults,
    {
        let len_results = results.len_results();
        results.call_results(&self.stack.values.as_slice()[..len_results])
    }
}
