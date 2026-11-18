// the .net testee for the shapekit conformance runner: answers testee protocol requests
// (docs/testee-protocol.md) with stellar-dotnet-sdk. one source, built once per pinned sdk
// version from the project file under versions/.

using System.Numerics;
using System.Reflection;
using System.Text;
using System.Text.Json;
using System.Text.Json.Nodes;
using StellarDotnetSdk;
using StellarDotnetSdk.Accounts;
using StellarDotnetSdk.Assets;
using StellarDotnetSdk.LiquidityPool;
using StellarDotnetSdk.Operations;
using StellarDotnetSdk.Soroban;
using StellarDotnetSdk.Transactions;
using StellarDotnetSdk.Xdr;
using Asset = StellarDotnetSdk.Assets.Asset;
using SCVal = StellarDotnetSdk.Soroban.SCVal;
using S = StellarDotnetSdk.Soroban;
using XdrScVal = StellarDotnetSdk.Xdr.SCVal;
using Transaction = StellarDotnetSdk.Transactions.Transaction;
using FeeBumpTransaction = StellarDotnetSdk.Transactions.FeeBumpTransaction;

namespace Shapekit.Testee;

public sealed class BadRequest(string m) : Exception(m);

public sealed class Unsupported(string m) : Exception(m);

/// a refusal the testee reports for the sdk, with the sdk's own words where there are some
public sealed class Refused(string m) : Exception(m);

/// what an sdk call threw, already sorted into rejected or panic by Ops.Sdk
public sealed class SdkFailure(string cls, string m) : Exception(m)
{
    public string Class { get; } = cls;
}

public static class Program
{
    // deep values recurse through the sdk's decoder and the request parser, so requests run
    // on a thread with a large stack
    private const int StackBytes = 512 << 20;

    public static int Main()
    {
        var status = 0;
        var worker = new Thread(() => status = Serve(), StackBytes);
        worker.Start();
        worker.Join();
        return status;
    }

    private static int Serve()
    {
        using var stdin = new StreamReader(Console.OpenStandardInput(), new UTF8Encoding(false));
        using var stdout = new StreamWriter(Console.OpenStandardOutput(), new UTF8Encoding(false));
        string? line;
        while ((line = stdin.ReadLine()) != null)
        {
            if (line.Length == 0) continue;
            stdout.Write(Ops.HandleLine(line).ToJsonString());
            stdout.Write('\n');
            stdout.Flush();
        }
        return 0;
    }
}

public static class Ops
{
    public static readonly string[] Supported =
    [
        "decode_encode", "strkey_decode", "strkey_encode", "asset_order", "pool_id", "id_parse",
        "to_native", "address_from_scval", "tx_hash", "balance_id", "contract_id",
    ];

    private static readonly JsonDocumentOptions Parse = new() { MaxDepth = 1 << 20 };

    /// an access to a null reference, a failed cast, an index past an array's end or running
    /// out of stack is the sdk crashing. every other exception is the sdk's refusal: it throws
    /// ArgumentException, InvalidOperationException, FormatException and its own exceptions on
    /// purpose for bad input.
    public static bool IsCrash(Exception e) =>
        e is NullReferenceException or InvalidCastException or IndexOutOfRangeException
            or InsufficientExecutionStackException;

    private static JsonObject Error(string id, string cls, string message) => new()
    {
        ["id"] = id, ["status"] = "error",
        ["error"] = new JsonObject { ["class"] = cls, ["message"] = message },
    };

    public static JsonObject HandleLine(string line)
    {
        JsonDocument doc;
        try
        {
            doc = JsonDocument.Parse(line, Parse);
        }
        catch (JsonException e)
        {
            return Error("", "bad-request", "request is not json: " + e.Message);
        }
        using var owned = doc;
        var req = doc.RootElement;
        if (req.ValueKind != JsonValueKind.Object || !req.TryGetProperty("id", out var idEl) ||
            idEl.ValueKind != JsonValueKind.String)
        {
            return Error("", "bad-request", "request is not an object with a string id");
        }
        var id = idEl.GetString()!;
        try
        {
            if (!req.TryGetProperty("op", out var op) || op.ValueKind != JsonValueKind.String)
                throw new BadRequest("request has no op");
            return new JsonObject { ["id"] = id, ["status"] = "ok", ["result"] = Run(op.GetString()!, req) };
        }
        catch (BadRequest e)
        {
            return Error(id, "bad-request", e.Message);
        }
        catch (Unsupported e)
        {
            return new JsonObject { ["id"] = id, ["status"] = "unsupported", ["reason"] = e.Message };
        }
        catch (Refused e)
        {
            return Error(id, "rejected", e.Message);
        }
        catch (SdkFailure e)
        {
            return Error(id, e.Class, e.Message);
        }
        catch (Exception e)
        {
            // not thrown inside an sdk call: the testee failed, the input says nothing about the sdk
            return Error(id, "internal", e.GetType().Name + ": " + e.Message);
        }
    }

    /// runs one call into the sdk and sorts what it throws: a crash (IsCrash) is panic, any other
    /// exception the sdk's refusal. only what escapes an sdk call is laid at the sdk's door; an
    /// exception from the testee's own code is internal (HandleLine)
    public static T Sdk<T>(Func<T> call)
    {
        try
        {
            return call();
        }
        catch (Exception e) when (e is not (BadRequest or Unsupported or Refused or SdkFailure))
        {
            var root = e;
            while (root is TargetInvocationException { InnerException: { } inner }) root = inner;
            var crash = IsCrash(root);
            // a crash names where it happened, the frame in the sdk the exception left
            var where = crash ? " " + (root.StackTrace ?? "").Split('\n').Select(l => l.Trim())
                .FirstOrDefault(l => l.Contains("StellarDotnetSdk"), "") : "";
            throw new SdkFailure(crash ? "panic" : "rejected", root.GetType().Name + ": " + root.Message + where);
        }
    }

    public static void Sdk(Action call) => Sdk<object?>(() =>
    {
        call();
        return null;
    });

    private static string SdkVersion =>
        typeof(StrKey).Assembly.GetCustomAttribute<AssemblyInformationalVersionAttribute>()!
            .InformationalVersion.Split('+')[0];

    private static JsonObject Run(string op, JsonElement req)
    {
        switch (op)
        {
            case "hello":
                return new JsonObject
                {
                    ["protocol"] = 2,
                    ["sdk"] = "stellar-dotnet-sdk",
                    ["sdk_version"] = SdkVersion,
                    ["components"] = new JsonObject
                    {
                        ["stellar-dotnet-sdk-xdr"] = typeof(XdrScVal).Assembly
                            .GetCustomAttribute<AssemblyInformationalVersionAttribute>()!.InformationalVersion.Split('+')[0],
                    },
                    ["ops"] = new JsonArray(Supported.Select(s => (JsonNode)s).ToArray()),
                    // Decode(stream) runs under the sdk's own XdrDataInputStream.DefaultMaxDepth
                    ["decode_limits"] = "ignored",
                };
            case "decode_encode":
            {
                var t = Tables.Of(Str(req, "type"));
                var value = Decode(t, Bytes(req, "xdr"));
                return new JsonObject { ["xdr"] = Convert.ToBase64String(Sdk(() => t.Encode(value))) };
            }
            case "decode_json":
            case "encode_json":
                throw new Unsupported("stellar-dotnet-sdk has no SEP-51 XDR-JSON reader or writer");
            case "strkey_decode": return StrkeyDecode(req);
            case "strkey_encode": return StrkeyEncode(req);
            case "asset_order":
            {
                var a = AssetOf(req, "a");
                var b = AssetOf(req, "b");
                var c = Sdk(() => a.CompareTo(b));
                return new JsonObject { ["order"] = Math.Sign(c) };
            }
            case "pool_id": return PoolId(req);
            case "id_parse": return IdParse(req);
            case "to_native":
            {
                var x = (XdrScVal)Decode(Tables.Of("ScVal"), Bytes(req, "xdr"));
                var val = Sdk(() => SCVal.FromXdr(x));
                return new JsonObject { ["native"] = Sdk(() => Describe(val)) };
            }
            case "address_from_scval":
            {
                var x = (XdrScVal)Decode(Tables.Of("ScVal"), Bytes(req, "xdr"));
                var addr = Sdk(() => ScAddress.FromSCValXdr(x));
                return new JsonObject { ["text"] = Sdk(() => AddressText(addr)) };
            }
            case "tx_hash": return TxHash(req);
            case "balance_id": return BalanceId(req);
            case "contract_id": return ContractId(req);
            case "map_sort":
            case "map_validate":
                throw new Unsupported("stellar-dotnet-sdk has no ScMap ordering or validation (SCMap in " +
                                      "StellarDotnetSdk/Soroban/SCVal.cs keeps entries in the order given)");
            case "coverage_snapshot":
            case "coverage_new":
                throw new Unsupported("the .net testee has no coverage build");
            default:
                throw new BadRequest("unknown op " + op);
        }
    }

    private static string Str(JsonElement req, string name) =>
        req.TryGetProperty(name, out var v) && v.ValueKind == JsonValueKind.String
            ? v.GetString()!
            : throw new BadRequest(name + " must be a string");

    private static byte[] Bytes(JsonElement req, string name)
    {
        var text = Str(req, name);
        // Convert.FromBase64String also skips white space, which the protocol refuses
        if (text.Length % 4 != 0 || text.Any(char.IsWhiteSpace))
            throw new BadRequest(name + " is not standard padded base64");
        try
        {
            return Convert.FromBase64String(text);
        }
        catch (FormatException e)
        {
            throw new BadRequest(name + " is not base64: " + e.Message);
        }
    }

    /// decodes the whole input: bytes left after the value are a refusal, as the protocol requires
    private static object Decode(Tables.XdrType t, byte[] bytes)
    {
        var stream = new XdrDataInputStream(bytes);
        var value = Sdk(() => t.Decode(stream));
        var left = Sdk(() => stream.GetRemainingInputLen());
        if (left != 0) throw new Refused(left + " trailing bytes after the value");
        return value;
    }

    private static JsonObject Kind(string kind, byte[] payload) =>
        new() { ["kind"] = kind, ["payload"] = Convert.ToBase64String(payload) };

    private static JsonObject StrkeyDecode(JsonElement req)
    {
        var hasText = req.TryGetProperty("text", out _);
        var hasB64 = req.TryGetProperty("text_base64", out _);
        string text;
        if (hasText && !hasB64) text = Str(req, "text");
        else if (hasB64 && !hasText)
        {
            try
            {
                text = new UTF8Encoding(false, true).GetString(Bytes(req, "text_base64"));
            }
            catch (DecoderFallbackException)
            {
                throw new Unsupported("the strkey decoders of stellar-dotnet-sdk take a .net string; bytes that " +
                                      "are not utf-8 have no string form");
            }
        }
        else throw new BadRequest("strkey_decode needs exactly one of text and text_base64");
        if (text.Length == 0) throw new Refused("empty strkey");
        switch (text[0])
        {
            case 'G': return Kind("account", Sdk(() => StrKey.DecodeEd25519PublicKey(text)));
            case 'M':
            {
                var m = Sdk(() => MuxedAccountMed25519.FromMuxedAccountId(text));
                var raw = new byte[40];
                Array.Copy(Sdk(() => m.PublicKey), raw, 32);
                var id = Sdk(() => m.Id);
                for (var i = 0; i < 8; i++) raw[32 + i] = (byte)(id >> (56 - 8 * i));
                return Kind("muxed_account", raw);
            }
            case 'S': return Kind("seed", Sdk(() => StrKey.DecodeEd25519SecretSeed(text)));
            case 'T': return Kind("pre_auth_tx", Sdk(() => StrKey.DecodePreAuthTx(text)));
            case 'X': return Kind("hash_x", Sdk(() => StrKey.DecodeSha256Hash(text)));
            case 'C': return Kind("contract", Sdk(() => StrKey.DecodeContractId(text)));
            case 'L': return Kind("liquidity_pool", Sdk(() => StrKey.DecodeLiquidityPoolId(text)));
            case 'B': return Kind("claimable_balance", Sdk(() => StrKey.DecodeClaimableBalanceId(text)));
            case 'W':
            {
                // the protocol: a first character with no decoder of the sdk's is a rejection
                var decode = typeof(StrKey).GetMethod("DecodeMuxedContractId", BindingFlags.Public | BindingFlags.Static, [typeof(string)]);
                if (decode == null) throw new Refused(SdkVersion + " has no strkey decoder for a string starting with W");
                return Kind("muxed_contract", (byte[])Sdk(() => decode.Invoke(null, [text]))!);
            }
            case 'P':
            {
                var p = Sdk(() => StrKey.DecodeSignedPayload(text));
                var payload = Sdk(() => p.Payload);
                var n = payload.Length;
                var raw = new List<byte>();
                raw.AddRange(Sdk(() => p.SignerAccountId.InnerValue.Ed25519.InnerValue));
                raw.AddRange([(byte)(n >> 24), (byte)(n >> 16), (byte)(n >> 8), (byte)n]);
                raw.AddRange(payload);
                raw.AddRange(new byte[(4 - n % 4) % 4]);
                return Kind("signed_payload", raw.ToArray());
            }
            default: throw new Refused("no strkey decoder for a string starting with " + text[0]);
        }
    }

    private static JsonObject StrkeyEncode(JsonElement req)
    {
        var kind = Str(req, "kind");
        var p = Bytes(req, "payload");
        string text = kind switch
        {
            "account" => Sdk(() => StrKey.EncodeEd25519PublicKey(p)),
            "muxed_account" => Sdk(() => StrKey.EncodeMed25519PublicKey(p)),
            "seed" => Sdk(() => StrKey.EncodeEd25519SecretSeed(p)),
            "pre_auth_tx" => Sdk(() => StrKey.EncodePreAuthTx(p)),
            "hash_x" => Sdk(() => StrKey.EncodeSha256Hash(p)),
            "contract" => Sdk(() => StrKey.EncodeContractId(p)),
            "liquidity_pool" => Sdk(() => StrKey.EncodeLiquidityPoolId(p)),
            "claimable_balance" => Sdk(() => StrKey.EncodeClaimableBalanceId(p)),
            "muxed_contract" => (string)Optional("EncodeMuxedContractId", p),
            "signed_payload" => EncodeSignedPayload(p),
            _ => throw new BadRequest("unknown strkey kind " + kind),
        };
        return new JsonObject { ["text"] = text };
    }

    /// a StrKey method that only later sdk versions have (the W strkey of CAP-84 came in 17.0.0),
    /// looked up by name so one source builds against every version
    private static object Optional(string name, object arg)
    {
        var m = typeof(StrKey).GetMethod(name, BindingFlags.Public | BindingFlags.Static, [arg.GetType()]);
        if (m == null) throw new Unsupported(SdkVersion + " has no StrKey." + name);
        return Sdk(() => m.Invoke(null, [arg])!);
    }

    // the encoder takes a SignedPayloadSigner: the key, then the payload its length names
    private static string EncodeSignedPayload(byte[] p)
    {
        if (p.Length < 36) throw new Refused("a signed payload needs a key and a length");
        var n = (p[32] << 24) | (p[33] << 16) | (p[34] << 8) | p[35];
        if (n < 0 || 36 + n > p.Length) throw new Refused("the payload length runs past the input");
        var padded = 36 + n + (4 - n % 4) % 4;
        if (p.Length != padded || p[(36 + n)..].Any(b => b != 0))
            throw new Unsupported("StrKey.EncodeSignedPayload takes the key and the payload; a raw payload with "
                + "padding that is not zero or bytes after it has no such form");
        return Sdk(() => StrKey.EncodeSignedPayload(new SignedPayloadSigner(p[..32], p[36..(36 + n)])));
    }

    private static Asset AssetOf(JsonElement req, string name)
    {
        var x = (StellarDotnetSdk.Xdr.Asset)Decode(Tables.Of("Asset"), Bytes(req, name));
        return Sdk(() => Asset.FromXdr(x));
    }

    private static JsonObject PoolId(JsonElement req)
    {
        var a = AssetOf(req, "a");
        var b = AssetOf(req, "b");
        if (!req.TryGetProperty("fee", out var f) || f.ValueKind != JsonValueKind.Number || !f.TryGetInt32(out var fee))
            throw new BadRequest("fee must be a 32-bit integer");
        var hash = Sdk(() => new LiquidityPoolId(LiquidityPoolType.LiquidityPoolTypeEnum.LIQUIDITY_POOL_CONSTANT_PRODUCT, a, b, fee).Hash);
        return new JsonObject { ["pool_id"] = Convert.ToHexString(hash).ToLowerInvariant() };
    }

    /// ManageSellOfferOperation takes the offer id as a long and MuxedAccountMed25519 the muxed
    /// id as a ulong, so a json number in range is handed over as that integer; text and
    /// numbers outside the range have no integer form
    private static JsonObject IdParse(JsonElement req)
    {
        var kind = Str(req, "kind");
        if (!req.TryGetProperty("input", out var input)) throw new BadRequest("id_parse needs member input");
        const string issuer = "GD7777777777777777777777777777777777777777777777777773DB";
        if (kind == "offer")
        {
            if (input.ValueKind != JsonValueKind.Number || !input.TryGetInt64(out var n))
                throw new Unsupported("ManageSellOfferOperation takes the offer id as a long only");
            var bytes = Sdk(() =>
            {
                var op = new ManageSellOfferOperation(new AssetTypeNative(),
                    new AssetTypeCreditAlphaNum4("ABC", issuer), "1", "1", n);
                var o = new XdrDataOutputStream();
                ManageSellOfferOp.Encode(o, op.ToOperationBody().ManageSellOfferOp);
                return o.ToArray();
            });
            // offerID is the op's last field (ManageSellOfferOp in Stellar-transaction.x)
            return new JsonObject { ["id"] = BigEndian(bytes, bytes.Length - 8, true) };
        }
        if (kind == "muxed")
        {
            if (input.ValueKind != JsonValueKind.Number || !input.TryGetUInt64(out var n))
                throw new Unsupported("MuxedAccountMed25519 takes the muxed id as a ulong only");
            var bytes = Sdk(() =>
            {
                var m = new MuxedAccountMed25519(KeyPair.FromAccountId(issuer), n);
                var o = new XdrDataOutputStream();
                StellarDotnetSdk.Xdr.MuxedAccount.Encode(o, m.MuxedAccount);
                return o.ToArray();
            });
            // KEY_TYPE_MUXED_ED25519: the type, then the uint64 id, then the key
            return new JsonObject { ["id"] = BigEndian(bytes, 4, false) };
        }
        throw new BadRequest("unknown id kind " + kind);
    }

    private static string BigEndian(byte[] b, int at, bool signed)
    {
        ulong v = 0;
        for (var i = 0; i < 8; i++) v = (v << 8) | b[at + i];
        return signed ? ((long)v).ToString() : v.ToString();
    }

    private static JsonObject Obj(string cls, string text) =>
        new() { ["type"] = "object", ["class"] = cls, ["text"] = text };

    private static JsonObject Int(BigInteger v) => new() { ["type"] = "integer", ["value"] = v.ToString() };

    /// the sdk's native form of a contract value is its SCVal class tree (SCVal.FromXdr,
    /// StellarDotnetSdk/Soroban/SCVal.cs); the testee describes each class's own values
    private static JsonNode Describe(SCVal v)
    {
        switch (v)
        {
            case SCVoid: return new JsonObject { ["type"] = "null" };
            case SCBool b: return new JsonObject { ["type"] = "bool", ["value"] = b.InnerValue };
            case SCUint32 x: return Int(x.InnerValue);
            case SCInt32 x: return Int(x.InnerValue);
            case SCUint64 x: return Int(x.InnerValue);
            case SCInt64 x: return Int(x.InnerValue);
            case SCUint128 x: return Int(((BigInteger)x.Hi << 64) | x.Lo);
            case SCInt128 x: return Int(((BigInteger)x.Hi << 64) | x.Lo);
            case SCUint256 x:
                return Int(((BigInteger)x.HiHi << 192) | ((BigInteger)x.HiLo << 128) | ((BigInteger)x.LoHi << 64) | x.LoLo);
            case SCInt256 x:
                return Int(((BigInteger)x.HiHi << 192) | ((BigInteger)x.HiLo << 128) | ((BigInteger)x.LoHi << 64) | x.LoLo);
            case S.SCBytes x: return new JsonObject { ["type"] = "bytes", ["value"] = Convert.ToBase64String(x.InnerValue) };
            case S.SCString x: return new JsonObject { ["type"] = "string", ["value"] = x.InnerValue };
            case S.SCSymbol x: return Obj(nameof(S.SCSymbol), x.InnerValue);
            case SCTimePoint x: return Obj(nameof(SCTimePoint), x.InnerValue.ToString());
            case SCDuration x: return Obj(nameof(SCDuration), x.InnerValue.ToString());
            case ScAddress a: return Obj(a.GetType().Name, AddressText(a));
            case S.SCVec x: return new JsonObject { ["type"] = "array", ["items"] = new JsonArray(x.InnerValue.Select(Describe).ToArray()) };
            case S.SCMap x:
                return new JsonObject
                {
                    ["type"] = "map",
                    ["entries"] = new JsonArray(x.Entries.Select(e => (JsonNode)new JsonObject
                    {
                        ["key"] = Describe(e.Key), ["value"] = Describe(e.Value),
                    }).ToArray()),
                };
            default:
                return Obj(v.GetType().Name, Convert.ToBase64String(Tables.Of("ScVal").Encode(v.ToXdr())));
        }
    }

    /// the strkey of an address, written with the sdk's encoders from the address the sdk's
    /// class converts back to xdr
    private static string AddressText(ScAddress a)
    {
        var x = a.ToXdr();
        return x.Discriminant.InnerValue switch
        {
            SCAddressType.SCAddressTypeEnum.SC_ADDRESS_TYPE_ACCOUNT =>
                StrKey.EncodeEd25519PublicKey(x.AccountId.InnerValue.Ed25519.InnerValue),
            SCAddressType.SCAddressTypeEnum.SC_ADDRESS_TYPE_CONTRACT =>
                StrKey.EncodeContractId(x.ContractId.InnerValue.InnerValue),
            SCAddressType.SCAddressTypeEnum.SC_ADDRESS_TYPE_MUXED_ACCOUNT => MuxedText(x.MuxedAccount),
            SCAddressType.SCAddressTypeEnum.SC_ADDRESS_TYPE_CLAIMABLE_BALANCE =>
                ClaimableBalanceIdUtils.FromXdr(x.ClaimableBalanceId),
            SCAddressType.SCAddressTypeEnum.SC_ADDRESS_TYPE_LIQUIDITY_POOL =>
                StrKey.EncodeLiquidityPoolId(x.LiquidityPoolId.InnerValue.InnerValue),
            _ => throw new Refused("an address of type " + x.Discriminant.InnerValue),
        };
    }

    private static string MuxedText(MuxedEd25519Account m)
    {
        var raw = new byte[40];
        Array.Copy(m.Ed25519.InnerValue, raw, 32);
        var id = m.Id.InnerValue;
        for (var i = 0; i < 8; i++) raw[32 + i] = (byte)(id >> (56 - 8 * i));
        return StrKey.EncodeMed25519PublicKey(raw);
    }

    private static Network NetworkOf(JsonElement req)
    {
        var passphrase = Str(req, "network_passphrase");
        return Sdk(() => new Network(passphrase));
    }

    private static TransactionEnvelope Envelope(JsonElement req) =>
        (TransactionEnvelope)Decode(Tables.Of("TransactionEnvelope"), Bytes(req, "xdr"));

    private static bool IsFeeBump(TransactionEnvelope e) =>
        e.Discriminant.InnerValue == EnvelopeType.EnvelopeTypeEnum.ENVELOPE_TYPE_TX_FEE_BUMP;

    private static JsonObject TxHash(JsonElement req)
    {
        var env = Envelope(req);
        var network = NetworkOf(req);
        var inner = req.TryGetProperty("inner", out var i) && i.ValueKind == JsonValueKind.True;
        if (req.TryGetProperty("inner", out var i2) && i2.ValueKind is not (JsonValueKind.True or JsonValueKind.False))
            throw new BadRequest("inner must be a boolean");
        byte[] hash;
        if (IsFeeBump(env))
        {
            hash = Sdk(() =>
            {
                var fb = FeeBumpTransaction.FromEnvelopeXdr(env);
                return inner ? fb.InnerTransaction.Hash(network) : fb.Hash(network);
            });
        }
        else
        {
            if (inner) throw new BadRequest("inner on an envelope that is no fee bump");
            hash = Sdk(() => Transaction.FromEnvelopeXdr(env).Hash(network));
        }
        return new JsonObject { ["hash"] = Convert.ToHexString(hash).ToLowerInvariant() };
    }

    /// the sdk has no balance id function, so the testee hashes the HashIDPreimage of the
    /// operation (ENVELOPE_TYPE_OP_ID) the sdk's own encoder writes, from the sdk's transaction
    private static JsonObject BalanceId(JsonElement req)
    {
        if (!req.TryGetProperty("op_index", out var ix) || ix.ValueKind != JsonValueKind.Number ||
            !ix.TryGetInt32(out var index) || index < 0)
            throw new BadRequest("op_index must be a non-negative integer");
        var env = Envelope(req);
        var tx = Sdk(() => IsFeeBump(env) ? FeeBumpTransaction.FromEnvelopeXdr(env).InnerTransaction : Transaction.FromEnvelopeXdr(env));
        var operations = Sdk(() => tx.Operations);
        if (index >= operations.Length) throw new Refused("no operation at index " + index);
        if (operations[index] is not CreateClaimableBalanceOperation)
            throw new Refused("the operation at index " + index + " does not create a claimable balance");
        var hash = Sdk(() =>
        {
            var preimage = new HashIDPreimage
            {
                Discriminant = EnvelopeType.Create(EnvelopeType.EnvelopeTypeEnum.ENVELOPE_TYPE_OP_ID),
                OperationID = new HashIDPreimage.HashIDPreimageOperationID
                {
                    SourceAccount = new AccountID(KeyPair.FromPublicKey(tx.SourceAccount.PublicKey).XdrPublicKey),
                    SeqNum = new SequenceNumber(new StellarDotnetSdk.Xdr.Int64(tx.SequenceNumber)),
                    OpNum = new Uint32((uint)index),
                },
            };
            var o = new XdrDataOutputStream();
            HashIDPreimage.Encode(o, preimage);
            return Util.Hash(o.ToArray());
        });
        return new JsonObject { ["balance_id"] = "00000000" + Convert.ToHexString(hash).ToLowerInvariant() };
    }

    /// the sdk has no contract id function, so the testee hashes the HashIDPreimage
    /// (ENVELOPE_TYPE_CONTRACT_ID) the sdk's own encoder writes
    private static JsonObject ContractId(JsonElement req)
    {
        var preimage = (ContractIDPreimage)Decode(Tables.Of("ContractIdPreimage"), Bytes(req, "preimage"));
        var network = NetworkOf(req);
        var hash = Sdk(() =>
        {
            var hp = new HashIDPreimage
            {
                Discriminant = EnvelopeType.Create(EnvelopeType.EnvelopeTypeEnum.ENVELOPE_TYPE_CONTRACT_ID),
                ContractID = new HashIDPreimage.HashIDPreimageContractID
                {
                    NetworkID = new Hash(network.NetworkId),
                    ContractIDPreimage = preimage,
                },
            };
            var o = new XdrDataOutputStream();
            HashIDPreimage.Encode(o, hp);
            return Util.Hash(o.ToArray());
        });
        return new JsonObject { ["contract_id"] = Convert.ToHexString(hash).ToLowerInvariant() };
    }
}

/// the sdk's xdr classes by the schema's type names: StellarDotnetSdk.Xdr names each class as
/// the .x file names the type (SCVal, AccountID), so a request's name matches the class whose
/// name equals it without regard to case. a type is usable when its class has the generated
/// static Decode(XdrDataInputStream) and Encode(XdrDataOutputStream, T).
public static class Tables
{
    public sealed record XdrType(Func<XdrDataInputStream, object> Decode, Func<object, byte[]> Encode);

    // where stellar-xdr's name is not the .x name: the overlay's Error is SError in stellar-xdr
    private static readonly Dictionary<string, string> Aliases = new() { ["serror"] = "error" };

    // the generator nests the anonymous unions and structs of a type inside its class
    // (TransactionResult+TransactionResultResult), under the name stellar-xdr gives them; a name
    // two classes share has no one class and stays out of the table
    private static readonly IEnumerable<IGrouping<string, Type>> Classes = typeof(XdrScVal).Assembly.GetTypes()
        .Where(t => t.Namespace == "StellarDotnetSdk.Xdr" && (t.IsPublic || t.IsNestedPublic))
        .GroupBy(t => t.Name.ToLowerInvariant());

    private static readonly Dictionary<string, Type> ByLowerName =
        Classes.Where(g => g.Count() == 1).ToDictionary(g => g.Key, g => g.Single());

    private static readonly Dictionary<string, string> Ambiguous =
        Classes.Where(g => g.Count() > 1).ToDictionary(g => g.Key, g => string.Join(", ", g.Select(t => t.FullName)));

    private static readonly Dictionary<string, XdrType> Cache = new();

    public static XdrType Of(string name)
    {
        var key = name.ToLowerInvariant();
        if (Aliases.TryGetValue(key, out var alias)) key = alias;
        if (Cache.TryGetValue(key, out var hit)) return hit;
        if (Ambiguous.TryGetValue(key, out var which))
            throw new Unsupported("stellar-dotnet-sdk has more than one class named " + name + ": " + which);
        if (!ByLowerName.TryGetValue(key, out var type))
            throw new Unsupported("stellar-dotnet-sdk has no class for the xdr type " + name);
        var decode = type.GetMethod("Decode", BindingFlags.Public | BindingFlags.Static, [typeof(XdrDataInputStream)]);
        var encode = type.GetMethod("Encode", BindingFlags.Public | BindingFlags.Static, [typeof(XdrDataOutputStream), type]);
        if (decode == null || encode == null)
            throw new Unsupported("stellar-dotnet-sdk class " + type.Name + " has no Decode and Encode");
        var t = new XdrType(
            s => decode.Invoke(null, [s])!,
            v =>
            {
                var o = new XdrDataOutputStream();
                encode.Invoke(null, [o, v]);
                return o.ToArray();
            });
        Cache[key] = t;
        return t;
    }
}
