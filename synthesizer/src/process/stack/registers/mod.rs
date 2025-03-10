// Copyright (C) 2019-2023 Aleo Systems Inc.
// This file is part of the snarkVM library.

// The snarkVM library is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// The snarkVM library is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with the snarkVM library. If not, see <https://www.gnu.org/licenses/>.

mod load;
mod store;

use crate::{CallStack, Operand, RegisterTypes, Stack};
use console::{
    network::prelude::*,
    program::{Entry, Literal, Plaintext, Register, Value},
    types::{Address, Field},
};

use indexmap::IndexMap;

#[derive(Clone)]
pub struct Registers<N: Network, A: circuit::Aleo<Network = N>> {
    /// The current call stack.
    call_stack: CallStack<N>,
    /// The mapping of all registers to their defined types.
    register_types: RegisterTypes<N>,
    /// The mapping of assigned console registers to their values.
    console_registers: IndexMap<u64, Value<N>>,
    /// The mapping of assigned circuit registers to their values.
    circuit_registers: IndexMap<u64, circuit::Value<A>>,
    /// The transition caller.
    caller: Option<Address<N>>,
    /// The transition caller, as a circuit.
    caller_circuit: Option<circuit::Address<A>>,
    /// The transition view key.
    tvk: Option<Field<N>>,
    /// The transition view key, as a circuit.
    tvk_circuit: Option<circuit::Field<A>>,
}

impl<N: Network, A: circuit::Aleo<Network = N>> Registers<N, A> {
    /// Initializes a new set of registers, given the call stack.
    #[inline]
    pub fn new(call_stack: CallStack<N>, register_types: RegisterTypes<N>) -> Self {
        Self {
            call_stack,
            register_types,
            console_registers: IndexMap::new(),
            circuit_registers: IndexMap::new(),
            caller: None,
            caller_circuit: None,
            tvk: None,
            tvk_circuit: None,
        }
    }

    /// Returns the current call stack.
    #[inline]
    pub fn call_stack(&self) -> CallStack<N> {
        self.call_stack.clone()
    }

    /// Returns the transition caller.
    #[inline]
    pub fn caller(&self) -> Result<Address<N>> {
        self.caller.ok_or_else(|| anyhow!("Caller address (console) is not set in the registers."))
    }

    /// Returns the transition caller, as a circuit.
    #[inline]
    pub fn caller_circuit(&self) -> Result<circuit::Address<A>> {
        self.caller_circuit.clone().ok_or_else(|| anyhow!("Caller address (circuit) is not set in the registers."))
    }

    /// Sets the transition caller.
    #[inline]
    pub fn set_caller(&mut self, caller: Address<N>) {
        self.caller = Some(caller);
    }

    /// Sets the transition caller, as a circuit.
    #[inline]
    pub fn set_caller_circuit(&mut self, caller_circuit: circuit::Address<A>) {
        self.caller_circuit = Some(caller_circuit);
    }

    /// Returns the transition view key.
    #[inline]
    pub fn tvk(&self) -> Result<Field<N>> {
        self.tvk.ok_or_else(|| anyhow!("Transition view key (console) is not set in the registers."))
    }

    /// Returns the transition view key, as a circuit.
    #[inline]
    pub fn tvk_circuit(&self) -> Result<circuit::Field<A>> {
        self.tvk_circuit.clone().ok_or_else(|| anyhow!("Transition view key (circuit) is not set in the registers."))
    }

    /// Sets the transition view key.
    #[inline]
    pub fn set_tvk(&mut self, tvk: Field<N>) {
        self.tvk = Some(tvk);
    }

    /// Sets the transition view key, as a circuit.
    #[inline]
    pub fn set_tvk_circuit(&mut self, tvk_circuit: circuit::Field<A>) {
        self.tvk_circuit = Some(tvk_circuit);
    }

    /// Ensure the console and circuit registers match.
    #[inline]
    pub fn ensure_console_and_circuit_registers_match(&self) -> Result<()> {
        use circuit::Eject;

        for ((console_index, console_register), (circuit_index, circuit_register)) in
            self.console_registers.iter().zip_eq(&self.circuit_registers)
        {
            // Ensure the console and circuit index match (executed in same order).
            if *console_index != *circuit_index {
                bail!("Console and circuit register indices are mismatching ({console_index} != {circuit_index})")
            }
            // Ensure the console and circuit registers match (executed to same value).
            if console_register != &circuit_register.eject_value() {
                bail!("The console and circuit register values do not match at index {console_index}")
            }
        }
        Ok(())
    }
}
