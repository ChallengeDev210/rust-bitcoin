// Rust Bitcoin Library
// Written in 2014 by
//     Andrew Poelstra <apoelstra@wpsoftware.net>
// To the extent possible under law, the author(s) have dedicated all
// copyright and related and neighboring rights to this software to
// the public domain worldwide. This software is distributed without
// any warranty.
//
// You should have received a copy of the CC0 Public Domain Dedication
// along with this software.
// If not, see <http://creativecommons.org/publicdomain/zero/1.0/>.
//

//! Addresses
//!
//! Support for ordinary base58 Bitcoin addresses and private keys
//!
//! # Example: creating a new address from a randomly-generated key pair
//!
//! ```rust
//! extern crate secp256k1;
//! extern crate bitcoin;
//!
//! use bitcoin::network::constants::Network;
//! use bitcoin::util::address::Address;
//! use bitcoin::util::key;
//! use secp256k1::Secp256k1;
//! use secp256k1::rand::thread_rng;
//!
//! fn main() {
//!     // Generate random key pair
//!     let s = Secp256k1::new();
//!     let public_key = key::PublicKey {
//!         compressed: true,
//!         key: s.generate_keypair(&mut thread_rng()).1,
//!     };
//!
//!     // Generate pay-to-pubkey-hash address
//!     let address = Address::p2pkh(&public_key, Network::Bitcoin);
//! }
//! ```

use std::fmt::{self, Display, Formatter};
use std::str::FromStr;

use bech32::{self, u5, FromBase32, ToBase32};
use bitcoin_hashes::{hash160, Hash};

use blockdata::opcodes;
use blockdata::script;
use network::constants::Network;
use util::base58;
use util::key;

/// Address error.
#[derive(Debug, PartialEq)]
pub enum Error {
    /// Base58 encoding error
    Base58(base58::Error),
    /// Bech32 encoding error
    Bech32(bech32::Error),
    /// The bech32 payload was empty
    EmptyBech32Payload,
    /// Script version must be 0 to 16 inclusive
    InvalidWitnessVersion(u8),
    /// The witness program must be between 2 and 40 bytes in length.
    InvalidWitnessProgramLength(usize),
    /// A v0 witness program must be either of length 20 or 32.
    InvalidSegwitV0ProgramLength(usize),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let desc = ::std::error::Error::description(self);
        match *self {
            Error::Base58(ref e) => write!(f, "{}: {}", desc, e),
            Error::Bech32(ref e) => write!(f, "{}: {}", desc, e),
            Error::InvalidWitnessVersion(v) => write!(f, "{}: {}", desc, v),
            Error::InvalidWitnessProgramLength(l) => write!(f, "{}: {}", desc, l),
            Error::InvalidSegwitV0ProgramLength(l) => write!(f, "{}: {}", desc, l),
            _ => f.write_str(desc),
        }
    }
}

impl ::std::error::Error for Error {
    fn cause(&self) -> Option<&::std::error::Error> {
        match *self {
            Error::Base58(ref e) => Some(e),
            Error::Bech32(ref e) => Some(e),
            _ => None,
        }
    }

    fn description(&self) -> &str {
        match *self {
            Error::Base58(..) => "base58 error",
            Error::Bech32(..) => "bech32 error",
            Error::EmptyBech32Payload => "the bech32 payload was empty",
            Error::InvalidWitnessVersion(..) => "invalid witness script version",
            Error::InvalidWitnessProgramLength(..) => {
                "the witness program must be between 2 and 40 bytes in length"
            },
            Error::InvalidSegwitV0ProgramLength(..) => {
                "a v0 witness program must be either of length 20 or 32"
            },
        }
    }
}

#[doc(hidden)]
impl From<base58::Error> for Error {
    fn from(e: base58::Error) -> Error {
        Error::Base58(e)
    }
}

#[doc(hidden)]
impl From<bech32::Error> for Error {
    fn from(e: bech32::Error) -> Error {
        Error::Bech32(e)
    }
}

/// The method used to produce an address
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Payload {
    /// pay-to-pkhash address
    PubkeyHash(hash160::Hash),
    /// P2SH address
    ScriptHash(hash160::Hash),
    /// Segwit address
    WitnessProgram {
        /// The witness program version
        version: u5,
        /// The witness program
        program: Vec<u8>,
    },
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
/// A Bitcoin address
pub struct Address {
    /// The type of the address
    pub payload: Payload,
    /// The network on which this address is usable
    pub network: Network,
}
serde_string_impl!(Address, "a Bitcoin address");

impl Address {
    /// Creates a pay to (compressed) public key hash address from a public key
    /// This is the preferred non-witness type address
    #[inline]
    pub fn p2pkh(pk: &key::PublicKey, network: Network) -> Address {
        let mut hash_engine = hash160::Hash::engine();
        pk.write_into(&mut hash_engine);

        Address {
            network: network,
            payload: Payload::PubkeyHash(hash160::Hash::from_engine(hash_engine))
        }
    }

    /// Creates a pay to script hash P2SH address from a script
    /// This address type was introduced with BIP16 and is the popular type to implement multi-sig these days.
    #[inline]
    pub fn p2sh(script: &script::Script, network: Network) -> Address {
        Address {
            network: network,
            payload: Payload::ScriptHash(hash160::Hash::hash(&script[..]))
        }
    }

    /// Create a witness pay to public key address from a public key
    /// This is the native segwit address type for an output redeemable with a single signature
    pub fn p2wpkh (pk: &key::PublicKey, network: Network) -> Address {
        let mut hash_engine = hash160::Hash::engine();
        pk.write_into(&mut hash_engine);

        Address {
            network: network,
            payload: Payload::WitnessProgram {
                version: u5::try_from_u8(0).expect("0<32"),
                program: hash160::Hash::from_engine(hash_engine)[..].to_vec(),
            },
        }
    }

    /// Create a pay to script address that embeds a witness pay to public key
    /// This is a segwit address type that looks familiar (as p2sh) to legacy clients
    pub fn p2shwpkh (pk: &key::PublicKey, network: Network) -> Address {
        let mut hash_engine = hash160::Hash::engine();
        pk.write_into(&mut hash_engine);

        let builder = script::Builder::new()
            .push_int(0)
            .push_slice(&hash160::Hash::from_engine(hash_engine)[..]);

        Address {
            network: network,
            payload: Payload::ScriptHash(
                hash160::Hash::hash(builder.into_script().as_bytes())
            )
        }
    }

    /// Create a witness pay to script hash address
    pub fn p2wsh (script: &script::Script, network: Network) -> Address {
        use bitcoin_hashes::sha256;

        Address {
            network: network,
            payload: Payload::WitnessProgram {
                version: u5::try_from_u8(0).expect("0<32"),
                program: sha256::Hash::hash(&script[..])[..].to_vec(),
            },
        }
    }

    /// Create a pay to script address that embeds a witness pay to script hash address
    /// This is a segwit address type that looks familiar (as p2sh) to legacy clients
    pub fn p2shwsh (script: &script::Script, network: Network) -> Address {
        use bitcoin_hashes::sha256;

        let ws = script::Builder::new().push_int(0)
                                       .push_slice(&sha256::Hash::hash(&script[..])[..])
                                       .into_script();

        Address {
            network: network,
            payload: Payload::ScriptHash(hash160::Hash::hash(&ws[..]))
        }
    }

    /// Check whether or not the address is following Bitcoin
    /// standardness rules.
    ///
    /// Segwit addresses with unassigned witness versions or non-standard
    /// program sizes are considered non-standard.
    pub fn is_standard(&self) -> bool {
        match self.payload {
            Payload::WitnessProgram {
                version: ver,
                program: ref prog,
            } => {
                // BIP-141 p2wpkh or p2wsh addresses.
                ver.to_u8() == 0 && (prog.len() == 20 || prog.len() == 32)
            }
            Payload::PubkeyHash(_) => true,
            Payload::ScriptHash(_) => true,
        }
    }

    /// Generates a script pubkey spending to this address
    pub fn script_pubkey(&self) -> script::Script {
        match self.payload {
            Payload::PubkeyHash(ref hash) => script::Builder::new()
                .push_opcode(opcodes::all::OP_DUP)
                .push_opcode(opcodes::all::OP_HASH160)
                .push_slice(&hash[..])
                .push_opcode(opcodes::all::OP_EQUALVERIFY)
                .push_opcode(opcodes::all::OP_CHECKSIG),
            Payload::ScriptHash(ref hash) => script::Builder::new()
                .push_opcode(opcodes::all::OP_HASH160)
                .push_slice(&hash[..])
                .push_opcode(opcodes::all::OP_EQUAL),
            Payload::WitnessProgram {
                version: ver,
                program: ref prog,
            } => script::Builder::new().push_int(ver.to_u8() as i64).push_slice(&prog),
        }
        .into_script()
    }
}

impl Display for Address {
    fn fmt(&self, fmt: &mut Formatter) -> fmt::Result {
        match self.payload {
            Payload::PubkeyHash(ref hash) => {
                let mut prefixed = [0; 21];
                prefixed[0] = match self.network {
                    Network::Bitcoin => 0,
                    Network::Testnet | Network::Regtest => 111,
                };
                prefixed[1..].copy_from_slice(&hash[..]);
                base58::check_encode_slice_to_fmt(fmt, &prefixed[..])
            },
            Payload::ScriptHash(ref hash) => {
                let mut prefixed = [0; 21];
                prefixed[0] = match self.network {
                    Network::Bitcoin => 5,
                    Network::Testnet | Network::Regtest => 196,
                };
                prefixed[1..].copy_from_slice(&hash[..]);
                base58::check_encode_slice_to_fmt(fmt, &prefixed[..])
            }
            Payload::WitnessProgram {
                version: ver,
                program: ref prog,
            } => {
                let mut b32_data = vec![ver];
                b32_data.extend_from_slice(&prog.to_base32());
                let hrp = match self.network {
                    Network::Bitcoin => "bc",
                    Network::Testnet => "tb",
                    Network::Regtest => "bcrt",
                };
                bech32::encode_to_fmt(fmt, &hrp, &b32_data).expect("only errors on invalid HRP")
            }
        }
    }
}

/// Extract the bech32 prefix.
/// Returns the same slice when no prefix is found.
fn find_bech32_prefix(bech32: &str) -> &str {
    // Split at the last occurrence of the separator character '1'.
    match bech32.rfind("1") {
        None => bech32,
        Some(sep) => bech32.split_at(sep).0,
    }
}

impl FromStr for Address {
    type Err = Error;

    fn from_str(s: &str) -> Result<Address, Error> {
        // try bech32
        let bech32_network = match find_bech32_prefix(s) {
            // note that upper or lowercase is allowed but NOT mixed case
            "bc" | "BC" => Some(Network::Bitcoin),
            "tb" | "TB" => Some(Network::Testnet),
            "bcrt" | "BCRT" => Some(Network::Regtest),
            _ => None,
        };
        if let Some(network) = bech32_network {
            // decode as bech32
            let (_, payload) = bech32::decode(s)?;
            if payload.len() == 0 {
                return Err(Error::EmptyBech32Payload);
            }

            // Get the script version and program (converted from 5-bit to 8-bit)
            let (version, program) = {
                let (v, p5) = payload.split_at(1);
                (v[0], Vec::from_base32(p5)?)
            };

            // Generic segwit checks.
            if version.to_u8() > 16 {
                return Err(Error::InvalidWitnessVersion(version.to_u8()));
            }
            if program.len() < 2 || program.len() > 40 {
                return Err(Error::InvalidWitnessProgramLength(program.len()));
            }

            // Specific segwit v0 check.
            if version.to_u8() == 0 && (program.len() != 20 && program.len() != 32) {
                return Err(Error::InvalidSegwitV0ProgramLength(program.len()));
            }

            return Ok(Address {
                payload: Payload::WitnessProgram {
                    version: version,
                    program: program,
                },
                network: network,
            });
        }

        // Base58
        if s.len() > 50 {
            return Err(Error::Base58(base58::Error::InvalidLength(s.len() * 11 / 15)));
        }
        let data = base58::from_check(s)?;
        if data.len() != 21 {
            return Err(Error::Base58(base58::Error::InvalidLength(data.len())));
        }

        let (network, payload) = match data[0] {
            0 => (
                Network::Bitcoin,
                Payload::PubkeyHash(hash160::Hash::from_slice(&data[1..]).unwrap())
            ),
            5 => (
                Network::Bitcoin,
                Payload::ScriptHash(hash160::Hash::from_slice(&data[1..]).unwrap())
            ),
            111 => (
                Network::Testnet,
                Payload::PubkeyHash(hash160::Hash::from_slice(&data[1..]).unwrap())
            ),
            196 => (
                Network::Testnet,
                Payload::ScriptHash(hash160::Hash::from_slice(&data[1..]).unwrap())
            ),
            x => return Err(Error::Base58(base58::Error::InvalidVersion(vec![x]))),
        };

        Ok(Address {
            network: network,
            payload: payload,
        })
    }
}

impl ::std::fmt::Debug for Address {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        write!(f, "{}", self.to_string())
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;
    use std::string::ToString;

    use bitcoin_hashes::{hash160, Hash};
    use hex::decode as hex_decode;

    use blockdata::script::Script;
    use network::constants::Network::{Bitcoin, Testnet, Regtest};
    use util::key::PublicKey;

    use super::*;

    macro_rules! hex (($hex:expr) => (hex_decode($hex).unwrap()));
    macro_rules! hex_key (($hex:expr) => (PublicKey::from_slice(&hex!($hex)).unwrap()));
    macro_rules! hex_script (($hex:expr) => (Script::from(hex!($hex))));
    macro_rules! hex_hash160 (($hex:expr) => (hash160::Hash::from_slice(&hex!($hex)).unwrap()));

    #[test]
    fn test_p2pkh_address_58() {
        let addr = Address {
            network: Bitcoin,
            payload: Payload::PubkeyHash(
                hex_hash160!("162c5ea71c0b23f5b9022ef047c4a86470a5b070")
            )
        };

        assert_eq!(addr.script_pubkey(), hex_script!("76a914162c5ea71c0b23f5b9022ef047c4a86470a5b07088ac"));
        assert_eq!(&addr.to_string(), "132F25rTsvBdp9JzLLBHP5mvGY66i1xdiM");
        assert_eq!(Address::from_str("132F25rTsvBdp9JzLLBHP5mvGY66i1xdiM").unwrap(), addr);
    }

    #[test]
    fn test_p2pkh_from_key() {
        let key = hex_key!("048d5141948c1702e8c95f438815794b87f706a8d4cd2bffad1dc1570971032c9b6042a0431ded2478b5c9cf2d81c124a5e57347a3c63ef0e7716cf54d613ba183");
        let addr = Address::p2pkh(&key, Bitcoin);
        assert_eq!(&addr.to_string(), "1QJVDzdqb1VpbDK7uDeyVXy9mR27CJiyhY");

        let key = hex_key!(&"03df154ebfcf29d29cc10d5c2565018bce2d9edbab267c31d2caf44a63056cf99f");
        let addr = Address::p2pkh(&key, Testnet);
        assert_eq!(&addr.to_string(), "mqkhEMH6NCeYjFybv7pvFC22MFeaNT9AQC");
    }

    #[test]
    fn test_p2sh_address_58() {
        let addr = Address {
            network: Bitcoin,
            payload: Payload::ScriptHash(
                hex_hash160!("162c5ea71c0b23f5b9022ef047c4a86470a5b070")
            )
        };

        assert_eq!(addr.script_pubkey(), hex_script!("a914162c5ea71c0b23f5b9022ef047c4a86470a5b07087"));
        assert_eq!(&addr.to_string(), "33iFwdLuRpW1uK1RTRqsoi8rR4NpDzk66k");
        assert_eq!(Address::from_str("33iFwdLuRpW1uK1RTRqsoi8rR4NpDzk66k").unwrap(), addr);
    }

    #[test]
    fn test_p2sh_parse() {
        let script = hex_script!("552103a765fc35b3f210b95223846b36ef62a4e53e34e2925270c2c7906b92c9f718eb2103c327511374246759ec8d0b89fa6c6b23b33e11f92c5bc155409d86de0c79180121038cae7406af1f12f4786d820a1466eec7bc5785a1b5e4a387eca6d797753ef6db2103252bfb9dcaab0cd00353f2ac328954d791270203d66c2be8b430f115f451b8a12103e79412d42372c55dd336f2eb6eb639ef9d74a22041ba79382c74da2338fe58ad21035049459a4ebc00e876a9eef02e72a3e70202d3d1f591fc0dd542f93f642021f82102016f682920d9723c61b27f562eb530c926c00106004798b6471e8c52c60ee02057ae");
        let addr = Address::p2sh(&script, Testnet);

        assert_eq!(&addr.to_string(), "2N3zXjbwdTcPsJiy8sUK9FhWJhqQCxA8Jjr");
        assert_eq!(Address::from_str("2N3zXjbwdTcPsJiy8sUK9FhWJhqQCxA8Jjr").unwrap(), addr);
    }

    #[test]
    fn test_p2wpkh () {
        // stolen from Bitcoin transaction: b3c8c2b6cfc335abbcb2c7823a8453f55d64b2b5125a9a61e8737230cdb8ce20
        let key = hex_key!("033bc8c83c52df5712229a2f72206d90192366c36428cb0c12b6af98324d97bfbc");
        let addr = Address::p2wpkh(&key, Bitcoin);
        assert_eq!(&addr.to_string(), "bc1qvzvkjn4q3nszqxrv3nraga2r822xjty3ykvkuw");
    }

    #[test]
    fn test_p2wsh () {
        // stolen from Bitcoin transaction 5df912fda4becb1c29e928bec8d64d93e9ba8efa9b5b405bd683c86fd2c65667
        let script = hex_script!("52210375e00eb72e29da82b89367947f29ef34afb75e8654f6ea368e0acdfd92976b7c2103a1b26313f430c4b15bb1fdce663207659d8cac749a0e53d70eff01874496feff2103c96d495bfdd5ba4145e3e046fee45e84a8a48ad05bd8dbb395c011a32cf9f88053ae");
        let addr = Address::p2wsh(&script, Bitcoin);
        assert_eq!(&addr.to_string(), "bc1qwqdg6squsna38e46795at95yu9atm8azzmyvckulcc7kytlcckxswvvzej");
    }

    #[test]
    fn test_bip173_vectors() {
        let addrstr = "BC1QW508D6QEJXTDG4Y5R3ZARVARY0C5XW7KV8F3T4";
        let addr = Address::from_str(addrstr).unwrap();
        assert_eq!(addr.network, Bitcoin);
        assert_eq!(addr.script_pubkey(), hex_script!("0014751e76e8199196d454941c45d1b3a323f1433bd6"));
        // skip round-trip because we'll serialize to lowercase which won't match

        let addrstr = "tb1qrp33g0q5c5txsp9arysrx4k6zdkfs4nce4xj0gdcccefvpysxf3q0sl5k7";
        let addr = Address::from_str(addrstr).unwrap();
        assert_eq!(addr.network, Testnet);
        assert_eq!(addr.script_pubkey(), hex_script!("00201863143c14c5166804bd19203356da136c985678cd4d27a1b8c6329604903262"));
        assert_eq!(addr.to_string(), addrstr);

        let addrstr = "tb1qqqqqp399et2xygdj5xreqhjjvcmzhxw4aywxecjdzew6hylgvsesrxh6hy";
        let addr = Address::from_str(addrstr).unwrap();
        assert_eq!(addr.network, Testnet);
        assert_eq!(addr.script_pubkey(), hex_script!("0020000000c4a5cad46221b2a187905e5266362b99d5e91c6ce24d165dab93e86433"));
        assert_eq!(addr.to_string(), addrstr);

        let addrstr = "bcrt1q2nfxmhd4n3c8834pj72xagvyr9gl57n5r94fsl";
        let addr = Address::from_str(addrstr).unwrap();
        assert_eq!(addr.network, Regtest);
        assert_eq!(addr.script_pubkey(), hex_script!("001454d26dddb59c7073c6a197946ea1841951fa7a74"));
        assert_eq!(addr.to_string(), addrstr);

        // bad vectors
        let addrstr = "tc1qw508d6qejxtdg4y5r3zarvary0c5xw7kg3g4ty"; // invalid hrp
        assert!(Address::from_str(addrstr).is_err());

        let addrstr = "bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t5"; // invalid checksum
        assert!(Address::from_str(addrstr).is_err());

        let addrstr = "BC13W508D6QEJXTDG4Y5R3ZARVARY0C5XW7KN40WF2"; // invalid witness version
        assert!(Address::from_str(addrstr).is_err());

        let addrstr = "bc1rw5uspcuh"; // invalid program length
        assert!(Address::from_str(addrstr).is_err());

        let addrstr = "bc10w508d6qejxtdg4y5r3zarvary0c5xw7kw508d6qejxtdg4y5r3zarvary0c5xw7kw5rljs90"; // invalid program length
        assert!(Address::from_str(addrstr).is_err());

        let addrstr = "BC1QR508D6QEJXTDG4Y5R3ZARVARYV98GJ9P"; // invalid program length for wit v0
        assert!(Address::from_str(addrstr).is_err());

        let addrstr = "tb1qrp33g0q5c5txsp9arysrx4k6zdkfs4nce4xj0gdcccefvpysxf3q0sL5k7"; // mixed case
        assert!(Address::from_str(addrstr).is_err());

        let addrstr = "bc1zw508d6qejxtdg4y5r3zarvaryvqyzf3du"; // zero padding of more than 4 bits
        assert!(Address::from_str(addrstr).is_err());

        let addrstr = "tb1qrp33g0q5c5txsp9arysrx4k6zdkfs4nce4xj0gdcccefvpysxf3pjxtptv"; // nonzero padding
        assert!(Address::from_str(addrstr).is_err());

        let addrstr = "bc1gmk9yu"; // empty data section
        assert!(Address::from_str(addrstr).is_err());
    }

    #[test]
    #[cfg(feature = "serde")]
    fn test_json_serialize() {
        use serde_json;

        let addr = Address::from_str("132F25rTsvBdp9JzLLBHP5mvGY66i1xdiM").unwrap();
        let json = serde_json::to_value(&addr).unwrap();
        assert_eq!(json, serde_json::Value::String("132F25rTsvBdp9JzLLBHP5mvGY66i1xdiM".to_owned()));
        let into: Address = serde_json::from_value(json).unwrap();
        assert_eq!(addr.to_string(), into.to_string());
        assert_eq!(
            into.script_pubkey(),
            hex_script!("76a914162c5ea71c0b23f5b9022ef047c4a86470a5b07088ac")
        );

        let addr = Address::from_str("33iFwdLuRpW1uK1RTRqsoi8rR4NpDzk66k").unwrap();
        let json = serde_json::to_value(&addr).unwrap();
        assert_eq!(json, serde_json::Value::String("33iFwdLuRpW1uK1RTRqsoi8rR4NpDzk66k".to_owned()));
        let into: Address = serde_json::from_value(json).unwrap();
        assert_eq!(addr.to_string(), into.to_string());
        assert_eq!(
            into.script_pubkey(),
            hex_script!("a914162c5ea71c0b23f5b9022ef047c4a86470a5b07087")
        );

        let addr = Address::from_str("tb1qrp33g0q5c5txsp9arysrx4k6zdkfs4nce4xj0gdcccefvpysxf3q0sl5k7").unwrap();
        let json = serde_json::to_value(&addr).unwrap();
        assert_eq!(json, serde_json::Value::String("tb1qrp33g0q5c5txsp9arysrx4k6zdkfs4nce4xj0gdcccefvpysxf3q0sl5k7".to_owned()));
        let into: Address = serde_json::from_value(json).unwrap();
        assert_eq!(addr.to_string(), into.to_string());
        assert_eq!(
            into.script_pubkey(),
            hex_script!("00201863143c14c5166804bd19203356da136c985678cd4d27a1b8c6329604903262")
        );

        let addr = Address::from_str("bcrt1q2nfxmhd4n3c8834pj72xagvyr9gl57n5r94fsl").unwrap();
        let json = serde_json::to_value(&addr).unwrap();
        assert_eq!(json, serde_json::Value::String("bcrt1q2nfxmhd4n3c8834pj72xagvyr9gl57n5r94fsl".to_owned()));
        let into: Address = serde_json::from_value(json).unwrap();
        assert_eq!(addr.to_string(), into.to_string());
        assert_eq!(
            into.script_pubkey(),
            hex_script!("001454d26dddb59c7073c6a197946ea1841951fa7a74")
        );
    }
}
