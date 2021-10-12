// Copyright (C) 2019-2021 Aleo Systems Inc.
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

use crate::{FunctionType, LedgerProof, LocalProof, Network, Record, Request, Response};
use snarkvm_algorithms::traits::EncryptionScheme;

use anyhow::Result;

#[derive(Derivative)]
#[derivative(Clone(bound = "N: Network"))]
pub struct InnerPrivateVariables<N: Network> {
    // Inputs.
    pub(super) input_records: Vec<Record<N>>,
    pub(super) ledger_proof: LedgerProof<N>,
    pub(super) local_proof: LocalProof<N>,
    pub(super) signature: N::AccountSignature,
    pub(super) function_type: FunctionType,
    // Outputs.
    pub(super) output_records: Vec<Record<N>>,
    pub(super) ciphertext_randomizers: Vec<<N::AccountEncryptionScheme as EncryptionScheme>::Randomness>,
}

impl<N: Network> InnerPrivateVariables<N> {
    pub(crate) fn blank() -> Self {
        Self {
            input_records: vec![Record::default(); N::NUM_INPUT_RECORDS],
            ledger_proof: Default::default(),
            local_proof: Default::default(),
            signature: N::AccountSignature::default(),
            function_type: FunctionType::Noop,
            output_records: vec![Record::default(); N::NUM_OUTPUT_RECORDS],
            ciphertext_randomizers: vec![
                <N::AccountEncryptionScheme as EncryptionScheme>::Randomness::default();
                N::NUM_OUTPUT_RECORDS
            ],
        }
    }

    pub(crate) fn new(request: &Request<N>, response: &Response<N>) -> Result<Self> {
        Ok(Self {
            input_records: request.records().clone(),
            ledger_proof: request.ledger_proof().clone(),
            local_proof: request.local_proof().clone(),
            signature: request.signature().clone(),
            function_type: request.function_type(),
            output_records: response.records().clone(),
            ciphertext_randomizers: response.ciphertext_randomizers().clone(),
        })
    }
}