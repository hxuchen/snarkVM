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

use crate::{crypto_hash::sha256::sha256, fft::EvaluationDomain, polycommit::kzg10, Prepare};
use hashbrown::HashMap;
use snarkvm_curves::{PairingCurve, PairingEngine, ProjectiveCurve};
use snarkvm_fields::{ConstraintFieldError, Field, PrimeField, ToConstraintField};
use snarkvm_utilities::{error, serialize::*, FromBytes, ToBytes};

use std::{
    borrow::{Borrow, Cow},
    collections::{BTreeMap, BTreeSet},
    fmt,
    ops::{AddAssign, MulAssign, SubAssign},
};

use super::{LabeledPolynomial, PolynomialInfo};

/// `UniversalParams` are the universal parameters for the KZG10 scheme.
pub type UniversalParams<E> = kzg10::UniversalParams<E>;

/// `Randomness` is the randomness for the KZG10 scheme.
pub type Randomness<E> = kzg10::KZGRandomness<E>;

/// `Commitment` is the commitment for the KZG10 scheme.
pub type Commitment<E> = kzg10::KZGCommitment<E>;

/// `PreparedCommitment` is the prepared commitment for the KZG10 scheme.
pub type PreparedCommitment<E> = kzg10::PreparedKZGCommitment<E>;

impl<E: PairingEngine> Prepare for Commitment<E> {
    type Prepared = PreparedCommitment<E>;

    /// prepare `PreparedCommitment` from `Commitment`
    fn prepare(&self) -> PreparedCommitment<E> {
        let mut prepared_comm = Vec::<E::G1Affine>::new();
        let mut cur = E::G1Projective::from(self.0);
        for _ in 0..128 {
            prepared_comm.push(cur.into());
            cur.double_in_place();
        }

        kzg10::PreparedKZGCommitment::<E>(prepared_comm)
    }
}

/// `CommitterKey` is used to commit to, and create evaluation proofs for, a given polynomial.
#[derive(Clone, Debug, Default, Hash, CanonicalSerialize, CanonicalDeserialize, PartialEq, Eq)]
pub struct CommitterKey<E: PairingEngine> {
    /// The key used to commit to polynomials.
    pub powers_of_beta_g: Vec<E::G1Affine>,

    /// The key used to commit to polynomials in Lagrange basis.
    pub lagrange_bases_at_beta_g: BTreeMap<usize, Vec<E::G1Affine>>,

    /// The key used to commit to hiding polynomials.
    pub powers_of_beta_times_gamma_g: Vec<E::G1Affine>,

    /// The powers used to commit to shifted polynomials.
    /// This is `None` if `self` does not support enforcing any degree bounds.
    pub shifted_powers_of_beta_g: Option<Vec<E::G1Affine>>,

    /// The powers used to commit to shifted hiding polynomials.
    /// This is `None` if `self` does not support enforcing any degree bounds.
    pub shifted_powers_of_beta_times_gamma_g: Option<BTreeMap<usize, Vec<E::G1Affine>>>,

    /// The degree bounds that are supported by `self`.
    /// Sorted in ascending order from smallest bound to largest bound.
    /// This is `None` if `self` does not support enforcing any degree bounds.
    pub enforced_degree_bounds: Option<Vec<usize>>,

    /// The maximum degree supported by the `UniversalParams` from which `self` was derived
    pub max_degree: usize,
}

impl<E: PairingEngine> FromBytes for CommitterKey<E> {
    fn read_le<R: Read>(mut reader: R) -> io::Result<Self> {
        // Deserialize `powers`.
        let powers_len: u32 = FromBytes::read_le(&mut reader)?;
        let mut powers_of_beta_g = Vec::with_capacity(powers_len as usize);
        for _ in 0..powers_len {
            let power: E::G1Affine = FromBytes::read_le(&mut reader)?;
            powers_of_beta_g.push(power);
        }

        // Deserialize `lagrange_basis_at_beta`.
        let lagrange_bases_at_beta_len: u32 = FromBytes::read_le(&mut reader)?;
        let mut lagrange_bases_at_beta_g = BTreeMap::new();
        for _ in 0..lagrange_bases_at_beta_len {
            let size: u32 = FromBytes::read_le(&mut reader)?;
            let mut basis = Vec::with_capacity(size as usize);
            for _ in 0..size {
                let power: E::G1Affine = FromBytes::read_le(&mut reader)?;
                basis.push(power);
            }
            lagrange_bases_at_beta_g.insert(size as usize, basis);
        }

        // Deserialize `powers_of_beta_times_gamma_g`.
        let powers_of_beta_times_gamma_g_len: u32 = FromBytes::read_le(&mut reader)?;
        let mut powers_of_beta_times_gamma_g = Vec::with_capacity(powers_of_beta_times_gamma_g_len as usize);
        for _ in 0..powers_of_beta_times_gamma_g_len {
            let powers_of_g: E::G1Affine = FromBytes::read_le(&mut reader)?;
            powers_of_beta_times_gamma_g.push(powers_of_g);
        }

        // Deserialize `shifted_powers_of_beta_g`.
        let has_shifted_powers_of_beta_g: bool = FromBytes::read_le(&mut reader)?;
        let shifted_powers_of_beta_g = match has_shifted_powers_of_beta_g {
            true => {
                let shifted_powers_len: u32 = FromBytes::read_le(&mut reader)?;
                let mut shifted_powers_of_beta_g = Vec::with_capacity(shifted_powers_len as usize);
                for _ in 0..shifted_powers_len {
                    let shifted_power: E::G1Affine = FromBytes::read_le(&mut reader)?;
                    shifted_powers_of_beta_g.push(shifted_power);
                }

                Some(shifted_powers_of_beta_g)
            }
            false => None,
        };

        // Deserialize `shifted_powers_of_beta_times_gamma_g`.
        let has_shifted_powers_of_beta_times_gamma_g: bool = FromBytes::read_le(&mut reader)?;
        let shifted_powers_of_beta_times_gamma_g = match has_shifted_powers_of_beta_times_gamma_g {
            true => {
                let mut shifted_powers_of_beta_times_gamma_g = BTreeMap::new();
                let shifted_powers_of_beta_times_gamma_g_num_elements: u32 = FromBytes::read_le(&mut reader)?;
                for _ in 0..shifted_powers_of_beta_times_gamma_g_num_elements {
                    let key: u32 = FromBytes::read_le(&mut reader)?;

                    let value_len: u32 = FromBytes::read_le(&mut reader)?;
                    let mut value = Vec::with_capacity(value_len as usize);
                    for _ in 0..value_len {
                        let val: E::G1Affine = FromBytes::read_le(&mut reader)?;
                        value.push(val);
                    }

                    shifted_powers_of_beta_times_gamma_g.insert(key as usize, value);
                }

                Some(shifted_powers_of_beta_times_gamma_g)
            }
            false => None,
        };

        // Deserialize `enforced_degree_bounds`.
        let has_enforced_degree_bounds: bool = FromBytes::read_le(&mut reader)?;
        let enforced_degree_bounds = match has_enforced_degree_bounds {
            true => {
                let enforced_degree_bounds_len: u32 = FromBytes::read_le(&mut reader)?;
                let mut enforced_degree_bounds = Vec::with_capacity(enforced_degree_bounds_len as usize);
                for _ in 0..enforced_degree_bounds_len {
                    let enforced_degree_bound: u32 = FromBytes::read_le(&mut reader)?;
                    enforced_degree_bounds.push(enforced_degree_bound as usize);
                }

                Some(enforced_degree_bounds)
            }
            false => None,
        };

        // Deserialize `max_degree`.
        let max_degree: u32 = FromBytes::read_le(&mut reader)?;

        // Construct the hash of the group elements.
        let mut hash_input = powers_of_beta_g.to_bytes_le().map_err(|_| error("Could not serialize powers"))?;

        hash_input.extend_from_slice(
            &powers_of_beta_times_gamma_g
                .to_bytes_le()
                .map_err(|_| error("Could not serialize powers_of_beta_times_gamma_g"))?,
        );

        if let Some(shifted_powers_of_beta_g) = &shifted_powers_of_beta_g {
            hash_input.extend_from_slice(
                &shifted_powers_of_beta_g
                    .to_bytes_le()
                    .map_err(|_| error("Could not serialize shifted_powers_of_beta_g"))?,
            );
        }

        if let Some(shifted_powers_of_beta_times_gamma_g) = &shifted_powers_of_beta_times_gamma_g {
            for value in shifted_powers_of_beta_times_gamma_g.values() {
                hash_input.extend_from_slice(
                    &value.to_bytes_le().map_err(|_| error("Could not serialize shifted_power_of_gamma_g"))?,
                );
            }
        }

        // Deserialize `hash`.
        let hash = sha256(&hash_input);
        let expected_hash: [u8; 32] = FromBytes::read_le(&mut reader)?;

        // Enforce the group elements construct the expected hash.
        if expected_hash != hash {
            return Err(error("Mismatching group elements"));
        }

        Ok(Self {
            powers_of_beta_g,
            lagrange_bases_at_beta_g,
            powers_of_beta_times_gamma_g,
            shifted_powers_of_beta_g,
            shifted_powers_of_beta_times_gamma_g,
            enforced_degree_bounds,
            max_degree: max_degree as usize,
        })
    }
}

impl<E: PairingEngine> ToBytes for CommitterKey<E> {
    fn write_le<W: Write>(&self, mut writer: W) -> io::Result<()> {
        // Serialize `powers`.
        (self.powers_of_beta_g.len() as u32).write_le(&mut writer)?;
        for power in &self.powers_of_beta_g {
            power.write_le(&mut writer)?;
        }

        // Serialize `powers`.
        (self.lagrange_bases_at_beta_g.len() as u32).write_le(&mut writer)?;
        for (size, powers) in &self.lagrange_bases_at_beta_g {
            (*size as u32).write_le(&mut writer)?;
            for power in powers {
                power.write_le(&mut writer)?;
            }
        }

        // Serialize `powers_of_beta_times_gamma_g`.
        (self.powers_of_beta_times_gamma_g.len() as u32).write_le(&mut writer)?;
        for power_of_gamma_g in &self.powers_of_beta_times_gamma_g {
            power_of_gamma_g.write_le(&mut writer)?;
        }

        // Serialize `shifted_powers_of_beta_g`.
        self.shifted_powers_of_beta_g.is_some().write_le(&mut writer)?;
        if let Some(shifted_powers_of_beta_g) = &self.shifted_powers_of_beta_g {
            (shifted_powers_of_beta_g.len() as u32).write_le(&mut writer)?;
            for shifted_power in shifted_powers_of_beta_g {
                shifted_power.write_le(&mut writer)?;
            }
        }

        // Serialize `shifted_powers_of_beta_times_gamma_g`.
        self.shifted_powers_of_beta_times_gamma_g.is_some().write_le(&mut writer)?;
        if let Some(shifted_powers_of_beta_times_gamma_g) = &self.shifted_powers_of_beta_times_gamma_g {
            (shifted_powers_of_beta_times_gamma_g.len() as u32).write_le(&mut writer)?;
            for (key, shifted_powers_of_beta_g) in shifted_powers_of_beta_times_gamma_g {
                (*key as u32).write_le(&mut writer)?;
                (shifted_powers_of_beta_g.len() as u32).write_le(&mut writer)?;
                for shifted_power in shifted_powers_of_beta_g {
                    shifted_power.write_le(&mut writer)?;
                }
            }
        }

        // Serialize `enforced_degree_bounds`.
        self.enforced_degree_bounds.is_some().write_le(&mut writer)?;
        if let Some(enforced_degree_bounds) = &self.enforced_degree_bounds {
            (enforced_degree_bounds.len() as u32).write_le(&mut writer)?;
            for enforced_degree_bound in enforced_degree_bounds {
                (*enforced_degree_bound as u32).write_le(&mut writer)?;
            }
        }

        // Serialize `max_degree`.
        (self.max_degree as u32).write_le(&mut writer)?;

        // Construct the hash of the group elements.
        let mut hash_input = self.powers_of_beta_g.to_bytes_le().map_err(|_| error("Could not serialize powers"))?;

        hash_input.extend_from_slice(
            &self
                .powers_of_beta_times_gamma_g
                .to_bytes_le()
                .map_err(|_| error("Could not serialize powers_of_beta_times_gamma_g"))?,
        );

        if let Some(shifted_powers_of_beta_g) = &self.shifted_powers_of_beta_g {
            hash_input.extend_from_slice(
                &shifted_powers_of_beta_g
                    .to_bytes_le()
                    .map_err(|_| error("Could not serialize shifted_powers_of_beta_g"))?,
            );
        }

        if let Some(shifted_powers_of_beta_times_gamma_g) = &self.shifted_powers_of_beta_times_gamma_g {
            for value in shifted_powers_of_beta_times_gamma_g.values() {
                hash_input.extend_from_slice(
                    &value.to_bytes_le().map_err(|_| error("Could not serialize shifted_power_of_gamma_g"))?,
                );
            }
        }

        // Serialize `hash`
        let hash = sha256(&hash_input);
        hash.write_le(&mut writer)
    }
}

impl<E: PairingEngine> CommitterKey<E> {
    /// Obtain powers for the underlying KZG10 construction
    pub fn powers(&self) -> kzg10::Powers<E> {
        kzg10::Powers {
            powers_of_beta_g: self.powers_of_beta_g.as_slice().into(),
            powers_of_beta_times_gamma_g: self.powers_of_beta_times_gamma_g.as_slice().into(),
        }
    }

    /// Obtain powers for committing to shifted polynomials.
    pub fn shifted_powers_of_beta_g(&self, degree_bound: impl Into<Option<usize>>) -> Option<kzg10::Powers<E>> {
        match (&self.shifted_powers_of_beta_g, &self.shifted_powers_of_beta_times_gamma_g) {
            (Some(shifted_powers_of_beta_g), Some(shifted_powers_of_beta_times_gamma_g)) => {
                let max_bound = self.enforced_degree_bounds.as_ref().unwrap().last().unwrap();
                let (bound, powers_range) = if let Some(degree_bound) = degree_bound.into() {
                    assert!(self.enforced_degree_bounds.as_ref().unwrap().contains(&degree_bound));
                    (degree_bound, (max_bound - degree_bound)..)
                } else {
                    (*max_bound, 0..)
                };

                let ck = kzg10::Powers {
                    powers_of_beta_g: shifted_powers_of_beta_g[powers_range].into(),
                    powers_of_beta_times_gamma_g: shifted_powers_of_beta_times_gamma_g[&bound].clone().into(),
                };

                Some(ck)
            }

            (_, _) => None,
        }
    }

    /// Obtain elements of the SRS in the lagrange basis powers, for use with the underlying
    /// KZG10 construction.
    pub fn lagrange_basis(&self, domain: EvaluationDomain<E::Fr>) -> Option<kzg10::LagrangeBasis<E>> {
        self.lagrange_bases_at_beta_g.get(&domain.size()).map(|basis| kzg10::LagrangeBasis {
            lagrange_basis_at_beta_g: Cow::Borrowed(basis),
            powers_of_beta_times_gamma_g: Cow::Borrowed(&self.powers_of_beta_times_gamma_g),
            domain,
        })
    }
}

impl<E: PairingEngine> CommitterKey<E> {
    pub fn max_degree(&self) -> usize {
        self.max_degree
    }

    pub fn supported_degree(&self) -> usize {
        self.powers_of_beta_g.len() - 1
    }
}

/// `VerifierKey` is used to check evaluation proofs for a given commitment.
#[derive(Clone, Debug, Default, PartialEq, Eq, CanonicalSerialize, CanonicalDeserialize)]
pub struct VerifierKey<E: PairingEngine> {
    /// The verification key for the underlying KZG10 scheme.
    pub vk: kzg10::VerifierKey<E>,

    /// Pairs a degree_bound with its corresponding G2 element.
    /// Each pair is in the form `(degree_bound, \beta^{degree_bound - max_degree} h),` where `h` is the generator of G2 above
    pub degree_bounds_and_neg_powers_of_h: Option<Vec<(usize, E::G2Affine)>>,

    /// The prepared version of `degree_bounds_and_neg_powers_of_h`.
    pub degree_bounds_and_prepared_neg_powers_of_h: Option<Vec<(usize, <E::G2Affine as PairingCurve>::Prepared)>>,

    /// The maximum degree supported by the trimmed parameters that `self` is
    /// a part of.
    pub supported_degree: usize,

    /// The maximum degree supported by the `UniversalParams` `self` was derived
    /// from.
    pub max_degree: usize,
}

impl<E: PairingEngine> FromBytes for VerifierKey<E> {
    fn read_le<R: Read>(mut reader: R) -> io::Result<Self> {
        CanonicalDeserialize::deserialize_compressed(&mut reader)
            .map_err(|_| error("could not deserialize VerifierKey"))
    }
}

impl<E: PairingEngine> ToBytes for VerifierKey<E> {
    fn write_le<W: Write>(&self, mut writer: W) -> io::Result<()> {
        CanonicalSerialize::serialize_compressed(self, &mut writer)
            .map_err(|_| error("could not serialize VerifierKey"))
    }
}

impl<E: PairingEngine> VerifierKey<E> {
    /// Find the appropriate shift for the degree bound.
    pub fn get_shift_power(&self, degree_bound: usize) -> Option<E::G2Affine> {
        self.degree_bounds_and_neg_powers_of_h
            .as_ref()
            .and_then(|v| v.binary_search_by(|(d, _)| d.cmp(&degree_bound)).ok().map(|i| v[i].1))
    }

    pub fn get_prepared_shift_power(&self, degree_bound: usize) -> Option<<E::G2Affine as PairingCurve>::Prepared> {
        self.degree_bounds_and_prepared_neg_powers_of_h
            .as_ref()
            .and_then(|v| v.binary_search_by(|(d, _)| d.cmp(&degree_bound)).ok().map(|i| v[i].1.clone()))
    }
}

impl<E: PairingEngine> VerifierKey<E> {
    pub fn max_degree(&self) -> usize {
        self.max_degree
    }

    pub fn supported_degree(&self) -> usize {
        self.supported_degree
    }
}

impl<E: PairingEngine> ToConstraintField<E::Fq> for VerifierKey<E> {
    fn to_field_elements(&self) -> Result<Vec<E::Fq>, ConstraintFieldError> {
        let mut res = Vec::new();
        res.extend_from_slice(&self.vk.to_field_elements()?);

        if let Some(degree_bounds_and_neg_powers_of_h) = &self.degree_bounds_and_neg_powers_of_h {
            for (d, neg_powers_of_h) in degree_bounds_and_neg_powers_of_h.iter() {
                let d_elem: E::Fq = (*d as u64).into();
                res.push(d_elem);
                res.append(&mut neg_powers_of_h.to_field_elements()?);
            }
        }

        Ok(res)
    }
}

/// `PreparedVerifierKey` is used to check evaluation proofs for a given commitment.
#[derive(Clone, Debug)]
pub struct PreparedVerifierKey<E: PairingEngine> {
    /// The verification key for the underlying KZG10 scheme.
    pub prepared_vk: kzg10::PreparedVerifierKey<E>,
    /// Information required to enforce degree bounds. Each pair
    /// is of the form `(degree_bound, shifting_advice)`.
    /// This is `None` if `self` does not support enforcing any degree bounds.
    pub degree_bounds_and_prepared_neg_powers_of_h: Option<Vec<(usize, <E::G2Affine as PairingCurve>::Prepared)>>,
    /// The maximum degree supported by the `UniversalParams` `self` was derived
    /// from.
    pub max_degree: usize,
    /// The maximum degree supported by the trimmed parameters that `self` is
    /// a part of.
    pub supported_degree: usize,
}

impl<E: PairingEngine> PreparedVerifierKey<E> {
    /// Find the appropriate shift for the degree bound.
    pub fn get_prepared_shift_power(&self, bound: usize) -> Option<<E::G2Affine as PairingCurve>::Prepared> {
        self.degree_bounds_and_prepared_neg_powers_of_h
            .as_ref()
            .and_then(|v| v.binary_search_by(|(d, _)| d.cmp(&bound)).ok().map(|i| v[i].1.clone()))
    }
}

impl<E: PairingEngine> Prepare for VerifierKey<E> {
    type Prepared = PreparedVerifierKey<E>;

    /// prepare `PreparedVerifierKey` from `VerifierKey`
    fn prepare(&self) -> PreparedVerifierKey<E> {
        let prepared_vk = kzg10::PreparedVerifierKey::<E>::prepare(&self.vk);

        PreparedVerifierKey::<E> {
            prepared_vk,
            degree_bounds_and_prepared_neg_powers_of_h: self.degree_bounds_and_prepared_neg_powers_of_h.clone(),
            max_degree: self.max_degree,
            supported_degree: self.supported_degree,
        }
    }
}

/// Evaluation proof at a query set.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, CanonicalSerialize, CanonicalDeserialize)]
pub struct BatchProof<E: PairingEngine>(pub(crate) Vec<kzg10::KZGProof<E>>);

impl<E: PairingEngine> BatchProof<E> {
    pub fn is_hiding(&self) -> bool {
        self.0.iter().any(|c| c.is_hiding())
    }
}

/// Labels a `LabeledPolynomial` or a `LabeledCommitment`.
pub type PolynomialLabel = String;

/// A commitment along with information about its degree bound (if any).
#[derive(Clone, Debug, CanonicalSerialize, PartialEq, Eq)]
pub struct LabeledCommitment<C: CanonicalSerialize + 'static> {
    label: PolynomialLabel,
    commitment: C,
    degree_bound: Option<usize>,
}

impl<F: Field, C: CanonicalSerialize + ToConstraintField<F>> ToConstraintField<F> for LabeledCommitment<C> {
    fn to_field_elements(&self) -> Result<Vec<F>, ConstraintFieldError> {
        self.commitment.to_field_elements()
    }
}

// NOTE: Serializing the LabeledCommitments struct is done by serializing
// _WITHOUT_ the labels or the degree bound. Deserialization is _NOT_ supported,
// and you should construct the struct via the `LabeledCommitment::new` method after
// deserializing the Commitment.
impl<C: CanonicalSerialize + ToBytes> ToBytes for LabeledCommitment<C> {
    fn write_le<W: Write>(&self, mut writer: W) -> io::Result<()> {
        CanonicalSerialize::serialize_compressed(&self.commitment, &mut writer)
            .map_err(|_| error("could not serialize struct"))
    }
}

impl<C: CanonicalSerialize> LabeledCommitment<C> {
    /// Instantiate a new polynomial_context.
    pub fn new(label: PolynomialLabel, commitment: C, degree_bound: Option<usize>) -> Self {
        Self { label, commitment, degree_bound }
    }

    pub fn new_with_info(info: &PolynomialInfo, commitment: C) -> Self {
        Self { label: info.label().to_string(), commitment, degree_bound: info.degree_bound() }
    }

    /// Return the label for `self`.
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Retrieve the commitment from `self`.
    pub fn commitment(&self) -> &C {
        &self.commitment
    }

    /// Retrieve the degree bound in `self`.
    pub fn degree_bound(&self) -> Option<usize> {
        self.degree_bound
    }
}

/// A term in a linear combination.
#[derive(Hash, Ord, PartialOrd, Clone, Eq, PartialEq)]
pub enum LCTerm {
    /// The constant term representing `one`.
    One,
    /// Label for a polynomial.
    PolyLabel(String),
}

impl fmt::Debug for LCTerm {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            LCTerm::One => write!(f, "1"),
            LCTerm::PolyLabel(label) => write!(f, "{label}"),
        }
    }
}

impl LCTerm {
    /// Returns `true` if `self == LCTerm::One`
    #[inline]
    pub fn is_one(&self) -> bool {
        matches!(self, LCTerm::One)
    }
}

impl From<PolynomialLabel> for LCTerm {
    fn from(other: PolynomialLabel) -> Self {
        Self::PolyLabel(other)
    }
}

impl<'a> From<&'a str> for LCTerm {
    fn from(other: &str) -> Self {
        Self::PolyLabel(other.into())
    }
}

impl core::convert::TryInto<PolynomialLabel> for LCTerm {
    type Error = ();

    fn try_into(self) -> Result<PolynomialLabel, ()> {
        match self {
            Self::One => Err(()),
            Self::PolyLabel(l) => Ok(l),
        }
    }
}

impl<'a> core::convert::TryInto<&'a PolynomialLabel> for &'a LCTerm {
    type Error = ();

    fn try_into(self) -> Result<&'a PolynomialLabel, ()> {
        match self {
            LCTerm::One => Err(()),
            LCTerm::PolyLabel(l) => Ok(l),
        }
    }
}

impl<B: Borrow<String>> PartialEq<B> for LCTerm {
    fn eq(&self, other: &B) -> bool {
        match self {
            Self::One => false,
            Self::PolyLabel(l) => l == other.borrow(),
        }
    }
}

/// A labeled linear combinations of polynomials.
#[derive(Clone, Debug)]
pub struct LinearCombination<F> {
    /// The label.
    pub label: String,
    /// The linear combination of `(coeff, poly_label)` pairs.
    pub terms: BTreeMap<LCTerm, F>,
}

#[allow(clippy::or_fun_call)]
impl<F: Field> LinearCombination<F> {
    /// Construct an empty labeled linear combination.
    pub fn empty(label: impl Into<String>) -> Self {
        Self { label: label.into(), terms: BTreeMap::new() }
    }

    /// Construct a new labeled linear combination.
    /// with the terms specified in `term`.
    pub fn new(label: impl Into<String>, _terms: impl IntoIterator<Item = (F, impl Into<LCTerm>)>) -> Self {
        let mut terms = BTreeMap::new();
        for (c, l) in _terms.into_iter().map(|(c, t)| (c, t.into())) {
            *terms.entry(l).or_insert(F::zero()) += c;
        }

        Self { label: label.into(), terms }
    }

    /// Returns the label of the linear combination.
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Returns `true` if the linear combination has no terms.
    pub fn is_empty(&self) -> bool {
        self.terms.is_empty()
    }

    /// Add a term to the linear combination.
    pub fn add(&mut self, c: F, t: impl Into<LCTerm>) -> &mut Self {
        let t = t.into();
        *self.terms.entry(t.clone()).or_insert(F::zero()) += c;
        if self.terms[&t].is_zero() {
            self.terms.remove(&t);
        }
        self
    }

    pub fn len(&self) -> usize {
        self.terms.len()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&F, &LCTerm)> {
        self.terms.iter().map(|(t, c)| (c, t))
    }
}

impl<'a, F: Field> AddAssign<(F, &'a LinearCombination<F>)> for LinearCombination<F> {
    #[allow(clippy::suspicious_op_assign_impl)]
    fn add_assign(&mut self, (coeff, other): (F, &'a LinearCombination<F>)) {
        for (t, c) in other.terms.iter() {
            self.add(coeff * c, t.clone());
        }
    }
}

impl<'a, F: Field> SubAssign<(F, &'a LinearCombination<F>)> for LinearCombination<F> {
    #[allow(clippy::suspicious_op_assign_impl)]
    fn sub_assign(&mut self, (coeff, other): (F, &'a LinearCombination<F>)) {
        for (t, c) in other.terms.iter() {
            self.add(-coeff * c, t.clone());
        }
    }
}

impl<'a, F: Field> AddAssign<&'a LinearCombination<F>> for LinearCombination<F> {
    fn add_assign(&mut self, other: &'a LinearCombination<F>) {
        for (t, c) in other.terms.iter() {
            self.add(*c, t.clone());
        }
    }
}

impl<'a, F: Field> SubAssign<&'a LinearCombination<F>> for LinearCombination<F> {
    fn sub_assign(&mut self, other: &'a LinearCombination<F>) {
        for (t, &c) in other.terms.iter() {
            self.add(-c, t.clone());
        }
    }
}

impl<F: Field> AddAssign<F> for LinearCombination<F> {
    fn add_assign(&mut self, coeff: F) {
        self.add(coeff, LCTerm::One);
    }
}

impl<F: Field> SubAssign<F> for LinearCombination<F> {
    fn sub_assign(&mut self, coeff: F) {
        self.add(-coeff, LCTerm::One);
    }
}

impl<F: Field> MulAssign<F> for LinearCombination<F> {
    fn mul_assign(&mut self, coeff: F) {
        self.terms.values_mut().for_each(|c| *c *= &coeff);
    }
}

/// `QuerySet` is the set of queries that are to be made to a set of labeled polynomials/equations
/// `p` that have previously been committed to. Each element of a `QuerySet` is a `(label, query)`
/// pair, where `label` is the label of a polynomial in `p`, and `query` is the field element
/// that `p[label]` is to be queried at.
///
/// Added the third field: the point name.
pub type QuerySet<'a, T> = BTreeSet<(String, (String, T))>;

/// `Evaluations` is the result of querying a set of labeled polynomials or equations
/// `p` at a `QuerySet` `Q`. It maps each element of `Q` to the resulting evaluation.
/// That is, if `(label, query)` is an element of `Q`, then `evaluation.get((label, query))`
/// should equal `p[label].evaluate(query)`.
pub type Evaluations<'a, F> = BTreeMap<(String, F), F>;

/// Evaluate the given polynomials at `query_set`.
pub fn evaluate_query_set<'a, F: PrimeField>(
    polys: impl IntoIterator<Item = &'a LabeledPolynomial<F>>,
    query_set: &QuerySet<'a, F>,
) -> Evaluations<'a, F> {
    let polys: HashMap<_, _> = polys.into_iter().map(|p| (p.label(), p)).collect();
    let mut evaluations = Evaluations::new();
    for (label, (_point_name, point)) in query_set {
        let poly = polys.get(label as &str).expect("polynomial in evaluated lc is not found");
        let eval = poly.evaluate(*point);
        evaluations.insert((label.clone(), *point), eval);
    }
    evaluations
}

/// A proof of satisfaction of linear combinations.
#[derive(Clone, Debug, PartialEq, Eq, CanonicalSerialize, CanonicalDeserialize)]
pub struct BatchLCProof<E: PairingEngine> {
    /// Evaluation proof.
    pub proof: BatchProof<E>,
    /// Evaluations required to verify the proof.
    pub evaluations: Option<Vec<E::Fr>>,
}

impl<E: PairingEngine> BatchLCProof<E> {
    pub fn is_hiding(&self) -> bool {
        self.proof.is_hiding()
    }
}

impl<E: PairingEngine> FromBytes for BatchLCProof<E> {
    fn read_le<R: Read>(mut reader: R) -> io::Result<Self> {
        CanonicalDeserialize::deserialize_compressed(&mut reader).map_err(|_| error("could not deserialize struct"))
    }
}

impl<E: PairingEngine> ToBytes for BatchLCProof<E> {
    fn write_le<W: Write>(&self, mut writer: W) -> io::Result<()> {
        CanonicalSerialize::serialize_compressed(self, &mut writer).map_err(|_| error("could not serialize struct"))
    }
}
