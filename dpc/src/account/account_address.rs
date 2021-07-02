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

use crate::{
    account_format,
    testnet1::{instantiated::Components, parameters::SystemParameters},
    traits::DPCComponents,
    AccountError,
    AccountPrivateKey,
    AccountViewKey,
    AddressError,
    Signature,
    ViewKey,
};
use snarkvm_algorithms::traits::EncryptionScheme;
use snarkvm_utilities::{FromBytes, ToBytes};

use bech32::{self, FromBase32, ToBase32};
use std::{
    fmt,
    io::{Read, Result as IoResult, Write},
    str::FromStr,
};

#[derive(Derivative)]
#[derivative(
    Default(bound = "C: DPCComponents"),
    Clone(bound = "C: DPCComponents"),
    PartialEq(bound = "C: DPCComponents"),
    Eq(bound = "C: DPCComponents")
)]
pub struct AccountAddress<C: DPCComponents> {
    pub encryption_key: <C::AccountEncryption as EncryptionScheme>::PublicKey,
}

#[derive(Debug)]
pub struct Address<C: DPCComponents> {
    pub(crate) address: AccountAddress<C>,
}

impl Address<Components> {
    pub fn from(private_key: &AccountPrivateKey<Components>) -> Result<Self, AddressError> {
        let parameters = SystemParameters::<Components>::load()?;
        let address = AccountAddress::<Components>::from_private_key(
            &parameters.account_signature,
            &parameters.account_commitment,
            &parameters.account_encryption,
            &private_key,
        )?;
        Ok(Self { address })
    }

    pub fn from_view_key(view_key: &ViewKey) -> Result<Self, AddressError> {
        let parameters = SystemParameters::<Components>::load()?;
        let address = AccountAddress::<Components>::from_view_key(&parameters.account_encryption, &view_key.view_key)?;
        Ok(Self { address })
    }

    /// Verify a signature signed by the view key
    /// Returns `true` if the signature is verified correctly. Otherwise, returns `false`.
    pub fn verify(&self, message: &[u8], signature: &Signature) -> Result<bool, AddressError> {
        let parameters = SystemParameters::<Components>::load()?;

        Ok(parameters
            .account_encryption
            .verify(&self.address.encryption_key, message, &signature.0)?)
    }
}

impl FromStr for Address<Components> {
    type Err = AddressError;

    fn from_str(address: &str) -> Result<Self, Self::Err> {
        Ok(Self {
            address: AccountAddress::<Components>::from_str(address)?,
        })
    }
}

impl fmt::Display for Address<Components> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.address.to_string())
    }
}

impl<C: DPCComponents> AccountAddress<C> {
    /// Derives the account address from an account private key.
    pub fn from_private_key(
        signature_parameters: &C::AccountSignature,
        commitment_parameters: &C::AccountCommitment,
        encryption_parameters: &C::AccountEncryption,
        private_key: &AccountPrivateKey<C>,
    ) -> Result<Self, AccountError> {
        let decryption_key = private_key.to_decryption_key(signature_parameters, commitment_parameters)?;
        let encryption_key =
            <C::AccountEncryption as EncryptionScheme>::generate_public_key(encryption_parameters, &decryption_key)?;

        Ok(Self { encryption_key })
    }

    /// Derives the account address from an account view key.
    pub fn from_view_key(
        encryption_parameters: &C::AccountEncryption,
        view_key: &AccountViewKey<C>,
    ) -> Result<Self, AccountError> {
        let encryption_key = <C::AccountEncryption as EncryptionScheme>::generate_public_key(
            encryption_parameters,
            &view_key.decryption_key,
        )?;

        Ok(Self { encryption_key })
    }

    #[allow(clippy::wrong_self_convention)]
    pub fn into_repr(&self) -> &<C::AccountEncryption as EncryptionScheme>::PublicKey {
        &self.encryption_key
    }
}

impl<C: DPCComponents> ToBytes for AccountAddress<C> {
    fn write<W: Write>(&self, mut writer: W) -> IoResult<()> {
        self.encryption_key.write(&mut writer)
    }
}

impl<C: DPCComponents> FromBytes for AccountAddress<C> {
    /// Reads in an account address buffer.
    #[inline]
    fn read<R: Read>(mut reader: R) -> IoResult<Self> {
        let encryption_key: <C::AccountEncryption as EncryptionScheme>::PublicKey = FromBytes::read(&mut reader)?;

        Ok(Self { encryption_key })
    }
}

impl<C: DPCComponents> FromStr for AccountAddress<C> {
    type Err = AccountError;

    /// Reads in an account address string.
    fn from_str(address: &str) -> Result<Self, Self::Err> {
        if address.len() != 63 {
            return Err(AccountError::InvalidCharacterLength(address.len()));
        }

        let prefix = &address.to_lowercase()[0..4];
        if prefix != account_format::ADDRESS_PREFIX {
            return Err(AccountError::InvalidPrefix(prefix.to_string()));
        };

        let (_hrp, data, _variant) = bech32::decode(&address)?;
        if data.is_empty() {
            return Err(AccountError::InvalidByteLength(0));
        }

        let buffer = Vec::from_base32(&data)?;
        Ok(Self::read(&buffer[..])?)
    }
}

impl<C: DPCComponents> fmt::Display for AccountAddress<C> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        // Write the encryption key to a buffer.
        let mut address = [0u8; 32];
        self.encryption_key
            .write(&mut address[0..32])
            .expect("address formatting failed");

        let prefix = account_format::ADDRESS_PREFIX.to_string();

        let result = bech32::encode(&prefix, address.to_base32(), bech32::Variant::Bech32);
        result.unwrap().fmt(f)
    }
}

impl<C: DPCComponents> fmt::Debug for AccountAddress<C> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "AccountAddress {{ encryption_key: {:?} }}", self.encryption_key)
    }
}
