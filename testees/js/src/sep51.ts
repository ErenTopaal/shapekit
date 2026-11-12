// sep-51 xdr-json (2.0.1) of a value decoded by the sdk's own js-xdr 4 classes, for the sdk
// releases that ship no xdr-json renderer (16.x). the walk reads js-xdr's type metadata:
// struct classes carry `structName` and `_fields`, union classes `unionName`, `_switchOn`,
// `_switches`, `_arms` and `_defaultArm`, enum classes `enumName` and `_members`, and the
// composite types are instances holding `_length`, `_maxLength` and `_childType`. js-xdr does
// not export its primitive and composite classes through the sdk, so they are learned from
// sdk types whose shape the xdr definitions fix (an ScVal's u32 arm is an unsigned int).
//
// strkeys are written with the sdk's own StrKey encoders; a strkey kind the sdk cannot encode
// makes the value unsupported rather than written by the testee.

import type { Json } from "./protocol.js";
import { Unsupported, sdk } from "./protocol.js";
import type { Loose } from "./sdk.js";

interface Primitives {
  int: unknown;
  uint: unknown;
  hyper: unknown;
  uhyper: unknown;
  bool: unknown;
  void: unknown;
  opaque: unknown;
  varOpaque: unknown;
  string: unknown;
  array: unknown;
  varArray: unknown;
  option: unknown;
}

function field(struct: Loose, name: string): Loose {
  const found = (struct._fields as [string, Loose][]).find(([n]) => n === name);
  if (found === undefined) {
    throw new Error(`${struct.structName} has no field ${name}`);
  }
  return found[1];
}

function primitives(xdr: Loose): Primitives {
  const arms = xdr.ScVal._arms;
  const voidMember = xdr.ScValType.scvVoid();
  return {
    int: arms.i32,
    uint: arms.u32,
    hyper: arms.i64,
    uhyper: arms.u64,
    bool: arms.b,
    void: xdr.ScVal._switches.get(voidMember),
    opaque: field(xdr.TtlEntry, "keyHash").constructor,
    varOpaque: arms.bytes.constructor,
    string: arms.str.constructor,
    array: field(xdr.LedgerHeader, "skipList").constructor,
    varArray: arms.vec._childType.constructor,
    option: arms.vec.constructor,
  };
}

// js-xdr turns the .x names into camelCase (SCV_U32 is scvU32, liquidityPoolID is
// liquidityPoolId); sep-51 wants them in snake_case
function snake(name: string): string {
  return name.replace(/[A-Z]/g, (c) => `_${c.toLowerCase()}`);
}

// js-xdr's code generator rewrites a few .x identifiers (txs becomes txes, IPv4 becomes iPv4,
// BINARY_FUSE_FILTER_8_BIT becomes binaryFuseFilter8Bit), so their .x names, which sep-51 is
// written in, cannot be read back from the sdk. these tables hold the sep-51 names for exactly
// those identifiers, taken from the v30 .x files.
const ENUM_MEMBER_NAMES: { [enumName: string]: { [member: string]: string } } = {
  BinaryFuseFilterType: {
    binaryFuseFilter8Bit: "b8_bit",
    binaryFuseFilter16Bit: "b16_bit",
    binaryFuseFilter32Bit: "b32_bit",
  },
  ConfigSettingId: { configSettingFreezeBypassTxes: "freeze_bypass_txs" },
  TransactionEventStage: {
    transactionEventStageBeforeAllTxes: "before_all_txs",
    transactionEventStageAfterAllTxes: "after_all_txs",
  },
};

// enums whose .x members (txSUCCESS, opINNER, IPv4) share no prefix that ends in an
// underscore, so sep-51 strips nothing, while their js names look as if they did
const ENUMS_WITHOUT_PREFIX = new Set(["TransactionResultCode", "OperationResultCode", "IpAddrType"]);

const STRUCT_FIELD_NAMES: { [structName: string]: { [field: string]: string } } = {
  FreezeBypassTxsDelta: { addTxes: "add_txs", removeTxes: "remove_txs" },
  LedgerCloseMetaBatch: { ledgerCloseMeta: "ledger_close_metas" },
  TransactionSet: { txes: "txs" },
  TxSetComponentTxsMaybeDiscountedFee: { txes: "txs" },
};

function fieldName(structName: string, field: string): string {
  return STRUCT_FIELD_NAMES[structName]?.[field] ?? snake(field);
}

// sep-51 enum names: snake_case with the prefix shared by all members removed, cut at an
// underscore, when the enum has more than one member
function enumNames(enumClass: Loose): Map<string, string> {
  const members = Object.keys(enumClass._members as object);
  const snaked = members.map(snake);
  const fixes = ENUM_MEMBER_NAMES[enumClass.enumName as string] ?? {};
  let prefix = "";
  if (snaked.length > 1 && !ENUMS_WITHOUT_PREFIX.has(enumClass.enumName as string)) {
    const first = snaked[0] ?? "";
    let end = first.length;
    for (const s of snaked) {
      let i = 0;
      while (i < end && i < s.length && s[i] === first[i]) {
        i++;
      }
      end = i;
    }
    const cut = first.lastIndexOf("_", end - 1);
    prefix = cut >= 0 ? first.slice(0, cut + 1) : "";
  }
  const out = new Map<string, string>();
  members.forEach((m, i) => out.set(m, fixes[m] ?? (snaked[i] ?? m).slice(prefix.length)));
  return out;
}

function hex(bytes: Uint8Array): string {
  return Buffer.from(bytes).toString("hex");
}

// sep-51 string escaping: printable ascii except backslash as is, \0 \t \n \r \\, and every
// other byte as \xNN
function escapeBytes(bytes: Uint8Array): string {
  let out = "";
  for (const b of bytes) {
    if (b === 0) out += "\\0";
    else if (b === 0x09) out += "\\t";
    else if (b === 0x0a) out += "\\n";
    else if (b === 0x0d) out += "\\r";
    else if (b === 0x5c) out += "\\\\";
    else if (b >= 0x20 && b <= 0x7e) out += String.fromCharCode(b);
    else out += `\\x${b.toString(16).padStart(2, "0")}`;
  }
  return out;
}

function trimZeros(bytes: Uint8Array, keep: number): Uint8Array {
  let end = bytes.length;
  while (end > keep && bytes[end - 1] === 0) {
    end--;
  }
  return bytes.subarray(0, end);
}

function be64(value: bigint): Buffer {
  const out = Buffer.alloc(8);
  out.writeBigUInt64BE(BigInt.asUintN(64, value));
  return out;
}

function bigOf(hyper: Loose): bigint {
  return BigInt(String(hyper));
}

// js-xdr resolves a typedef to its target, so ContractID and PoolID arrive as plain Hash
// opaques. in the v30 .x files every ContractID sits in a field or arm named contractID or
// contractId and every PoolID in one named liquidityPoolID or liquidityPoolId, and no Hash of
// another meaning uses those names, so the name says which strkey sep-51 wants
const CONTRACT_ID_NAMES = new Set(["contractId", "contractID"]);
const POOL_ID_NAMES = new Set(["liquidityPoolId", "liquidityPoolID"]);

export class Sep51 {
  private readonly xdr: Loose;
  private readonly strKey: Loose;
  private readonly p: Primitives;
  private readonly enumCache = new Map<unknown, Map<string, string>>();

  constructor(module: Loose) {
    this.xdr = module.xdr;
    this.strKey = module.StrKey;
    this.p = primitives(this.xdr);
  }

  render(type: Loose, value: Loose): Json {
    return this.walk(type, value, "");
  }

  private names(enumClass: Loose): Map<string, string> {
    let names = this.enumCache.get(enumClass);
    if (names === undefined) {
      names = enumNames(enumClass);
      this.enumCache.set(enumClass, names);
    }
    return names;
  }

  private is(type: Loose, ...names: string[]): boolean {
    return names.some((n) => this.xdr[n] !== undefined && this.xdr[n] === type);
  }

  private strkey(encoder: string, bytes: Uint8Array): string {
    const encode = this.strKey[encoder];
    if (typeof encode !== "function") {
      throw new Unsupported(`StrKey.${encoder} does not exist in this sdk version`);
    }
    return sdk(() => encode.call(this.strKey, Buffer.from(bytes)) as string);
  }

  private walk(type: Loose, value: Loose, name: string): Json {
    const p = this.p;
    if (type === p.int || type === p.uint) {
      return value as number;
    }
    if (type === p.hyper || type === p.uhyper) {
      return String(value);
    }
    if (type === p.bool) {
      return value as boolean;
    }
    if (typeof type === "function") {
      if (typeof type.structName === "string") return this.struct(type, value);
      if (typeof type.unionName === "string") return this.union(type, value);
      if (typeof type.enumName === "string") return this.enumValue(type, value);
    }
    if (type instanceof (p.opaque as Loose)) return this.opaque(type, value, name);
    if (type instanceof (p.varOpaque as Loose)) return hex(value);
    if (type instanceof (p.string as Loose)) return escapeBytes(value);
    if (type instanceof (p.array as Loose) || type instanceof (p.varArray as Loose)) {
      return (value as Loose[]).map((v) => this.walk(type._childType, v, name));
    }
    if (type instanceof (p.option as Loose)) {
      return value === undefined || value === null ? null : this.walk(type._childType, value, name);
    }
    throw new Error(`no sep-51 rule for js-xdr type ${describeType(type)} at ${name || "the root"}`);
  }

  private opaque(type: Loose, value: Uint8Array, name: string): Json {
    if (this.is(type, "AssetCode4")) return escapeBytes(trimZeros(value, 0));
    if (this.is(type, "AssetCode12")) return escapeBytes(trimZeros(value, 5));
    if (value.length === 32 && CONTRACT_ID_NAMES.has(name)) {
      return this.strkey("encodeContract", value);
    }
    if (value.length === 32 && POOL_ID_NAMES.has(name)) {
      return this.strkey("encodeLiquidityPool", value);
    }
    return hex(value);
  }

  private enumValue(type: Loose, value: Loose): Json {
    const rendered = this.names(type).get(value.name);
    if (rendered === undefined) {
      throw new Error(`${type.enumName} has no member ${value.name}`);
    }
    return rendered;
  }

  private struct(type: Loose, value: Loose): Json {
    const attrs = value._attributes;
    if (this.is(type, "Int128Parts")) {
      return ((bigOf(attrs.hi) << 64n) | BigInt.asUintN(64, bigOf(attrs.lo))).toString();
    }
    if (this.is(type, "UInt128Parts")) {
      return ((BigInt.asUintN(64, bigOf(attrs.hi)) << 64n) | BigInt.asUintN(64, bigOf(attrs.lo))).toString();
    }
    if (this.is(type, "Int256Parts", "UInt256Parts")) {
      const top = this.is(type, "Int256Parts") ? bigOf(attrs.hiHi) : BigInt.asUintN(64, bigOf(attrs.hiHi));
      const rest = [attrs.hiLo, attrs.loHi, attrs.loLo].map((h: Loose) => BigInt.asUintN(64, bigOf(h)));
      return rest.reduce((acc: bigint, part: bigint) => (acc << 64n) | part, top).toString();
    }
    if (this.is(type, "MuxedAccountMed25519", "MuxedEd25519Account")) {
      return this.strkey("encodeMed25519PublicKey", Buffer.concat([attrs.ed25519, be64(bigOf(attrs.id))]));
    }
    if (this.is(type, "MuxedContract")) {
      return this.strkey("encodeMuxedContract", Buffer.concat([attrs.contractId, be64(bigOf(attrs.id))]));
    }
    if (this.is(type, "SignerKeyEd25519SignedPayload")) {
      // the sep-23 payload of a P strkey is this struct's own xdr
      return this.strkey("encodeSignedPayload", sdk(() => value.toXDR()));
    }
    const out: { [key: string]: Json } = {};
    for (const [jsName, fieldType] of type._fields as [string, Loose][]) {
      out[fieldName(type.structName as string, jsName)] = this.walk(fieldType, attrs[jsName], jsName);
    }
    return out;
  }

  private union(type: Loose, value: Loose): Json {
    const arm = value.arm();
    const inner = value.value();
    if (this.is(type, "PublicKey")) {
      return this.strkey("encodeEd25519PublicKey", inner);
    }
    if (this.is(type, "MuxedAccount", "SignerKey")) {
      switch (arm) {
        case "ed25519":
          return this.strkey("encodeEd25519PublicKey", inner);
        case "preAuthTx":
          return this.strkey("encodePreAuthTx", inner);
        case "hashX":
          return this.strkey("encodeSha256Hash", inner);
        default:
          // med25519 and ed25519SignedPayload are structs with their own rules
          return this.walk(type._arms[arm], inner, arm);
      }
    }
    if (this.is(type, "ClaimableBalanceId")) {
      // the B payload is the id type byte, then the hash
      const typeByte = Buffer.from([value.switch().value]);
      return this.strkey("encodeClaimableBalance", Buffer.concat([typeByte, inner]));
    }
    if (this.is(type, "ScAddress", "AssetCode")) {
      // these render as the arm's value alone, without the union's tag
      return this.walk(type._arms[arm], inner, arm);
    }
    const tag = this.unionTag(type, value.switch());
    if (arm === this.p.void) {
      return tag;
    }
    return { [tag]: this.walk(type._arms[arm], inner, arm) };
  }

  private unionTag(type: Loose, discriminant: Loose): string {
    const switchOn = type._switchOn;
    if (switchOn === this.p.int || switchOn === this.p.uint) {
      return `v${String(discriminant)}`;
    }
    return this.enumValue(switchOn, discriminant) as string;
  }
}

function describeType(type: Loose): string {
  if (typeof type === "function") return `class ${type.name as string}`;
  if (type !== null && typeof type === "object") return `instance of ${type.constructor?.name as string}`;
  return String(type);
}
