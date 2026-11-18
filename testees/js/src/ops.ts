// the testee protocol operations on @stellar/stellar-sdk top-level exports. one source runs
// against every release in the matrix: where releases differ the op checks for the api it
// needs, and answers unsupported when the release has none.

import type { Json } from "./protocol.js";
import type { Coverage } from "./coverage.js";
import { BadRequest, PROTOCOL_VERSION, SdkError, Unsupported, sdk } from "./protocol.js";
import { describeNative } from "./native.js";
import { Sep51 } from "./sep51.js";
import type { LoadedSdk, Loose } from "./sdk.js";
import { SDK_PACKAGE } from "./sdk.js";

export interface Request {
  op: string;
  members: { [key: string]: unknown };
  // the json text of the `input` member when it is a number, so id_parse sees the digits the
  // runner wrote and not a rounded double
  inputSource: string | undefined;
}

type Result = { [key: string]: Json };

// the base account of muxed ids: 32 bytes of 0x3a, the key the textual-id vectors use
const MUXED_BASE = "GA5DUOR2HI5DUOR2HI5DUOR2HI5DUOR2HI5DUOR2HI5DUOR2HI5DVTUJ";

const STRKEY_KINDS: { [kind: string]: { prefix: string; codec: string } } = {
  account: { prefix: "G", codec: "Ed25519PublicKey" },
  muxed_account: { prefix: "M", codec: "Med25519PublicKey" },
  seed: { prefix: "S", codec: "Ed25519SecretSeed" },
  pre_auth_tx: { prefix: "T", codec: "PreAuthTx" },
  hash_x: { prefix: "X", codec: "Sha256Hash" },
  signed_payload: { prefix: "P", codec: "SignedPayload" },
  contract: { prefix: "C", codec: "Contract" },
  liquidity_pool: { prefix: "L", codec: "LiquidityPool" },
  claimable_balance: { prefix: "B", codec: "ClaimableBalance" },
  muxed_contract: { prefix: "W", codec: "MuxedContract" },
};

function str(req: Request, name: string): string {
  const v = req.members[name];
  if (typeof v !== "string") {
    throw new BadRequest(`${req.op} needs string member ${name}`);
  }
  return v;
}

// standard base64 with padding: groups of four alphabet characters, the last group may end in
// "=" or "==". checked in one pass because a backtracking regex over the base64 of a large
// ledger (about 5 MB) runs out of stack in v8
function isBase64(text: string): boolean {
  if (text.length % 4 !== 0) return false;
  const padding = text.endsWith("==") ? 2 : text.endsWith("=") ? 1 : 0;
  for (let i = 0; i < text.length - padding; i++) {
    const c = text.charCodeAt(i);
    const alphabet =
      (c >= 65 && c <= 90) || (c >= 97 && c <= 122) || (c >= 48 && c <= 57) || c === 43 || c === 47;
    if (!alphabet) return false;
  }
  return true;
}

function base64(text: string, what: string): Buffer {
  if (!isBase64(text)) {
    throw new BadRequest(`${what} is not standard base64`);
  }
  return Buffer.from(text, "base64");
}

function bytes(req: Request, name: string): Buffer {
  return base64(str(req, name), name);
}

// text, or text_base64 for input that is not utf-8. StrKey decodes javascript strings only, and
// bytes that are not utf-8 have no faithful string form, so they are unsupported rather than
// mapped to text the testee picks.
function strkeyText(req: Request): string {
  const hasText = "text" in req.members;
  const hasBytes = "text_base64" in req.members;
  if (hasText === hasBytes) {
    throw new BadRequest("strkey_decode needs exactly one of text and text_base64");
  }
  if (hasText) {
    return str(req, "text");
  }
  const raw = bytes(req, "text_base64");
  try {
    return new TextDecoder("utf-8", { fatal: true }).decode(raw);
  } catch {
    throw new Unsupported("StrKey decodes javascript strings, which cannot hold bytes that are not utf-8");
  }
}

export class Ops {
  private readonly m: Loose;
  private readonly xdr: Loose;
  private readonly loaded: LoadedSdk;
  // 17.x generates its own XdrValue classes with fromXdr, toXdr and toJson; 16.x uses
  // js-xdr 4 classes with fromXDR and toXDR only
  private readonly xdrValues: boolean;
  private sep51: Sep51 | undefined;

  private readonly coverage: Coverage | undefined;

  constructor(loaded: LoadedSdk, coverage?: Coverage) {
    this.loaded = loaded;
    this.coverage = coverage?.forPackage(loaded.dir);
    this.m = loaded.module;
    this.xdr = this.m.xdr;
    this.xdrValues = typeof this.xdr.ScVal.fromXdr === "function" && this.xdr.ScVal.schema !== undefined;
  }

  // every op this sdk version has an api for
  supported(): string[] {
    const ops = ["decode_encode", "decode_json"];
    if (this.xdrValues) ops.push("encode_json");
    ops.push("strkey_decode", "strkey_encode");
    if (typeof this.m.Asset?.compare === "function") ops.push("asset_order");
    if (typeof this.m.getLiquidityPoolId === "function") ops.push("pool_id");
    if (typeof this.m.scvSortedMap === "function") ops.push("map_sort");
    ops.push("id_parse");
    if (typeof this.m.scValToNative === "function") ops.push("to_native");
    if (typeof this.m.Address?.fromScVal === "function") ops.push("address_from_scval");
    if (typeof this.m.TransactionBuilder?.fromXDR === "function") ops.push("tx_hash");
    if (typeof this.m.Transaction?.prototype?.getClaimableBalanceId === "function") ops.push("balance_id");
    if (typeof this.m.Asset?.prototype?.contractId === "function" && typeof this.m.hash === "function") {
      ops.push("contract_id");
    }
    if (this.coverage) ops.push("coverage_snapshot", "coverage_new");
    return ops;
  }

  run(req: Request): Result {
    switch (req.op) {
      case "hello":
        return this.hello();
      case "decode_encode":
        return this.decodeEncode(req);
      case "decode_json":
        return this.decodeJson(req);
      case "encode_json":
        return this.encodeJson(req);
      case "strkey_decode":
        return this.strkeyDecode(req);
      case "strkey_encode":
        return this.strkeyEncode(req);
      case "asset_order":
        return this.assetOrder(req);
      case "pool_id":
        return this.poolId(req);
      case "map_sort":
        return this.mapSort(req);
      case "map_validate":
        throw new Unsupported(
          `${SDK_PACKAGE} ${this.loaded.version} exports no ScMap key comparator or map validation; ` +
            "scvSortedMap sorts with a comparator it keeps private",
        );
      case "id_parse":
        return this.idParse(req);
      case "to_native":
        return this.toNative(req);
      case "address_from_scval":
        return this.addressFromScVal(req);
      case "tx_hash":
        return this.txHash(req);
      case "balance_id":
        return this.balanceId(req);
      case "contract_id":
        return this.contractId(req);
      case "coverage_snapshot":
        if (!this.coverage) {
          throw new Unsupported("coverage is collected only when the testee runs with SHAPEKIT_COVERAGE=1");
        }
        return { tool: `v8 ${process.versions.v8} precise block coverage`, ...this.coverage.snapshot() };
      case "coverage_new":
        if (!this.coverage) {
          throw new Unsupported("coverage is collected only when the testee runs with SHAPEKIT_COVERAGE=1");
        }
        return this.coverage.fresh();
      default:
        throw new BadRequest(`unknown op ${req.op}`);
    }
  }

  private hello(): Result {
    return {
      protocol: PROTOCOL_VERSION,
      sdk: SDK_PACKAGE,
      sdk_version: this.loaded.version,
      components: this.loaded.components,
      ops: this.supported(),
      // XdrValue.fromXdr (17.x) and fromXDR (16.x) take no limits. js-xdr's own maxDepth counts
      // every schema entered, which is not the depth the runner's limit means, so the sdk
      // decodes with its defaults
      decode_limits: "ignored",
    };
  }

  private xdrType(name: string): Loose {
    // the sdk spells a few types differently from the .x files (Uint128Parts for
    // UInt128Parts); a name that matches one export ignoring case is that type
    let key: string | undefined = Object.hasOwn(this.xdr, name) ? name : undefined;
    if (key === undefined) {
      const matches = Object.keys(this.xdr).filter((k) => k.toLowerCase() === name.toLowerCase());
      if (matches.length === 1) key = matches[0];
    }
    const type = key === undefined ? undefined : this.xdr[key];
    const decodes = this.xdrValues ? typeof type?.fromXdr === "function" : typeof type?.fromXDR === "function";
    if (!decodes) {
      throw new Unsupported(`${SDK_PACKAGE} ${this.loaded.version} has no xdr type ${name}`);
    }
    return type;
  }

  private decodeAs(type: Loose, input: Buffer): Loose {
    return sdk(() => (this.xdrValues ? type.fromXdr(new Uint8Array(input)) : type.fromXDR(input)));
  }

  private encode(value: Loose): string {
    const out: Uint8Array = sdk(() => (this.xdrValues ? value.toXdr() : value.toXDR()));
    return Buffer.from(out).toString("base64");
  }

  private decodeRequest(req: Request): { type: Loose; value: Loose } {
    const type = this.xdrType(str(req, "type"));
    return { type, value: this.decodeAs(type, bytes(req, "xdr")) };
  }

  private decodeEncode(req: Request): Result {
    const { value } = this.decodeRequest(req);
    return { xdr: this.encode(value) };
  }

  private decodeJson(req: Request): Result {
    const { type, value } = this.decodeRequest(req);
    if (this.xdrValues) {
      return { json: sdk(() => value.toJson() as Json) };
    }
    this.sep51 ??= new Sep51(this.m);
    return { json: this.sep51.render(type, value) };
  }

  private encodeJson(req: Request): Result {
    if (!this.xdrValues) {
      throw new Unsupported(`${SDK_PACKAGE} ${this.loaded.version} has no xdr-json reader`);
    }
    const type = this.xdrType(str(req, "type"));
    if (!Object.hasOwn(req.members, "json")) {
      throw new BadRequest("encode_json needs member json");
    }
    const value = sdk(() => type.fromJson(req.members["json"]));
    return { xdr: this.encode(value) };
  }

  private strkeyDecode(req: Request): Result {
    const text = strkeyText(req);
    const entry = Object.entries(STRKEY_KINDS).find(([, k]) => text.startsWith(k.prefix));
    if (entry === undefined) {
      throw new SdkError(new Error(`no StrKey decoder for a string starting with ${JSON.stringify(text.slice(0, 1))}`));
    }
    const [kind, { codec }] = entry;
    const decode = this.m.StrKey[`decode${codec}`];
    if (typeof decode !== "function") {
      // the protocol counts a missing decoder for a prefix as the sdk refusing the string
      throw new SdkError(new Error(`StrKey.decode${codec} does not exist in this sdk version`));
    }
    const raw: Uint8Array = sdk(() => decode.call(this.m.StrKey, text));
    return { kind, payload: Buffer.from(raw).toString("base64") };
  }

  private strkeyEncode(req: Request): Result {
    const kind = str(req, "kind");
    const payload = bytes(req, "payload");
    const known = STRKEY_KINDS[kind];
    if (known === undefined) {
      throw new BadRequest(`unknown strkey kind ${kind}`);
    }
    const encode = this.m.StrKey[`encode${known.codec}`];
    if (typeof encode !== "function") {
      throw new Unsupported(`${SDK_PACKAGE} ${this.loaded.version} has no StrKey.encode${known.codec}`);
    }
    return { text: sdk(() => encode.call(this.m.StrKey, payload) as string) };
  }

  private asset(req: Request, name: string): Loose {
    const xdrAsset = this.decodeAs(this.xdrType("Asset"), bytes(req, name));
    return sdk(() => this.m.Asset.fromOperation(xdrAsset));
  }

  private assetOrder(req: Request): Result {
    const a = this.asset(req, "a");
    const b = this.asset(req, "b");
    const order: number = sdk(() => this.m.Asset.compare(a, b));
    return { order: Math.sign(order) };
  }

  private poolId(req: Request): Result {
    const fee = req.members["fee"];
    if (typeof fee !== "number" || !Number.isInteger(fee)) {
      throw new BadRequest("pool_id needs integer member fee");
    }
    // the pair goes in as given: getLiquidityPoolId refuses pairs it does not see in order
    const assetA = this.asset(req, "a");
    const assetB = this.asset(req, "b");
    const id: Uint8Array = sdk(() => this.m.getLiquidityPoolId("constant_product", { assetA, assetB, fee }));
    return { pool_id: Buffer.from(id).toString("hex") };
  }

  // the transaction the sdk builds from an envelope; a fee bump's inner one when asked
  private transaction(req: Request, inner: boolean): Loose {
    const passphrase = str(req, "network_passphrase");
    const envelope = bytes(req, "xdr").toString("base64");
    const tx: Loose = sdk(() => this.m.TransactionBuilder.fromXDR(envelope, passphrase));
    const isFeeBump = tx instanceof this.m.FeeBumpTransaction;
    if (!inner) return tx;
    if (!isFeeBump) throw new BadRequest("inner is only defined for a fee bump envelope");
    return tx.innerTransaction;
  }

  private txHash(req: Request): Result {
    const inner = req.members["inner"] === true;
    const tx = this.transaction(req, inner);
    const h: Uint8Array = sdk(() => tx.hash());
    return { hash: Buffer.from(h).toString("hex") };
  }

  private balanceId(req: Request): Result {
    const index = req.members["op_index"];
    if (typeof index !== "number" || !Number.isInteger(index)) {
      throw new BadRequest("balance_id needs integer member op_index");
    }
    let tx = this.transaction(req, false);
    // the id comes from the transaction that holds the operation, the inner one of a fee bump
    if (tx instanceof this.m.FeeBumpTransaction) tx = tx.innerTransaction;
    const id: string = sdk(() => tx.getClaimableBalanceId(index));
    return { balance_id: id.toLowerCase() };
  }

  // Asset.contractId is the sdk's api for an asset's contract; a preimage from an address
  // has no api of its own, so its id is the sdk's hash of the sdk's encoding of the
  // HashIdPreimage, the way Asset.contractId builds it
  private contractId(req: Request): Result {
    const passphrase = str(req, "network_passphrase");
    const preimage = this.decodeAs(this.xdrType("ContractIdPreimage"), bytes(req, "preimage"));
    // 17.x unions carry their arm as `type` and the value as a field; 16.x js-xdr unions as
    // switch() and an accessor method
    const arm: string = this.xdrValues ? preimage.type : sdk(() => preimage.switch().name);
    if (arm === "contractIdPreimageFromAsset") {
      const xdrAsset = this.xdrValues ? preimage.fromAsset : sdk(() => preimage.fromAsset());
      const asset = sdk(() => this.m.Asset.fromOperation(xdrAsset));
      const id: string = sdk(() => asset.contractId(passphrase));
      return { contract_id: Buffer.from(sdk(() => this.m.StrKey.decodeContract(id))).toString("hex") };
    }
    const networkId: Uint8Array = sdk(() => this.m.hash(Buffer.from(passphrase, "utf8")));
    const full = sdk(() =>
      this.xdr.HashIdPreimage.envelopeTypeContractId(
        new this.xdr.HashIdPreimageContractId({ networkId, contractIdPreimage: preimage }),
      ),
    );
    const encoded: Uint8Array = sdk(() => (this.xdrValues ? full.toXdr() : full.toXDR()));
    const id: Uint8Array = sdk(() => this.m.hash(encoded));
    return { contract_id: Buffer.from(id).toString("hex") };
  }

  private mapSort(req: Request): Result {
    const entries = req.members["entries"];
    if (!Array.isArray(entries)) {
      throw new BadRequest("map_sort needs array member entries");
    }
    const scVal = this.xdrType("ScVal");
    const items = entries.map((e: unknown, i) => {
      if (typeof e !== "object" || e === null) {
        throw new BadRequest(`entries[${i}] is not an object`);
      }
      const { key, val } = e as { key?: unknown; val?: unknown };
      if (typeof key !== "string" || typeof val !== "string") {
        throw new BadRequest(`entries[${i}] needs string members key and val`);
      }
      const k = this.decodeAs(scVal, base64(key, `entries[${i}].key`));
      const v = this.decodeAs(scVal, base64(val, `entries[${i}].val`));
      return sdk(() => new this.xdr.ScMapEntry({ key: k, val: v }));
    });
    const map = sdk(() => this.m.scvSortedMap(items));
    return { xdr: this.encode(map) };
  }

  private idParse(req: Request): Result {
    const kind = str(req, "kind");
    if (!Object.hasOwn(req.members, "input")) {
      throw new BadRequest("id_parse needs member input");
    }
    const input = req.members["input"];
    let arg: string | number;
    if (typeof input === "string") {
      arg = input;
    } else if (typeof input === "number" && req.inputSource !== undefined) {
      // the sdk takes a javascript number, so the digits go through a double exactly as a
      // caller's literal would
      arg = Number(req.inputSource);
    } else {
      throw new BadRequest("id_parse input must be a string or a number");
    }
    switch (kind) {
      case "muxed":
        return { id: this.muxedId(arg) };
      case "offer":
        return { id: this.offerId(arg) };
      default:
        throw new BadRequest(`unknown id kind ${kind}`);
    }
  }

  private muxedId(arg: string | number): string {
    if (typeof arg !== "string") {
      throw new Unsupported("MuxedAccount takes the id only as a string");
    }
    const muxed = sdk(() => new this.m.MuxedAccount(new this.m.Account(MUXED_BASE, "0"), arg));
    // the id the network sees is the one in the M address, which is not always what id()
    // echoes back
    const address: string = sdk(() => muxed.accountId());
    const raw: Uint8Array = sdk(() => this.m.StrKey.decodeMed25519PublicKey(address));
    return Buffer.from(raw).readBigUInt64BE(32).toString();
  }

  private offerId(arg: string | number): string {
    const op = sdk(() =>
      this.m.Operation.manageSellOffer({
        selling: this.m.Asset.native(),
        buying: new this.m.Asset("USD", MUXED_BASE),
        amount: "1",
        price: "1",
        offerId: arg,
      }),
    );
    // ManageSellOfferOp ends with the int64 offer id, so the operation's xdr does too
    const raw: Uint8Array = sdk(() => (this.xdrValues ? op.toXdr() : op.toXDR()));
    return Buffer.from(raw).readBigInt64BE(raw.length - 8).toString();
  }

  private toNative(req: Request): Result {
    const value = this.decodeAs(this.xdrType("ScVal"), bytes(req, "xdr"));
    const native: unknown = sdk(() => this.m.scValToNative(value));
    return { native: describeNative(native) };
  }

  // Address.fromScVal throws for an ScVal that is not an address; toString is its strkey
  private addressFromScVal(req: Request): Result {
    const value = this.decodeAs(this.xdrType("ScVal"), bytes(req, "xdr"));
    const address: Loose = sdk(() => this.m.Address.fromScVal(value));
    const text: unknown = sdk(() => address.toString());
    if (typeof text !== "string") throw new Error(`Address.toString returned ${typeof text}`);
    return { text };
  }
}
