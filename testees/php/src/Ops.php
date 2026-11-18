<?php declare(strict_types=1);

namespace Shapekit\Testee;

use Composer\InstalledVersions;
use Soneso\StellarSDK\AbstractTransaction;
use Soneso\StellarSDK\Asset;
use Soneso\StellarSDK\AssetTypeCreditAlphanum4;
use Soneso\StellarSDK\AssetTypePoolShare;
use Soneso\StellarSDK\Crypto\KeyPair;
use Soneso\StellarSDK\Crypto\StrKey;
use Soneso\StellarSDK\FeeBumpTransaction;
use Soneso\StellarSDK\ManageSellOfferOperationBuilder;
use Soneso\StellarSDK\MuxedAccount;
use Soneso\StellarSDK\Network;
use Soneso\StellarSDK\Soroban\Address;
use Soneso\StellarSDK\StellarSDK;
use Soneso\StellarSDK\Transaction;
use Soneso\StellarSDK\Xdr\XdrAccountID;
use Soneso\StellarSDK\Xdr\XdrBuffer;
use Soneso\StellarSDK\Xdr\XdrContractIDPreimage;
use Soneso\StellarSDK\Xdr\XdrContractIDPreimageType;
use Soneso\StellarSDK\Xdr\XdrEnvelopeType;
use Soneso\StellarSDK\Xdr\XdrHashIDPreimage;
use Soneso\StellarSDK\Xdr\XdrHashIDPreimageContractID;
use Soneso\StellarSDK\Xdr\XdrHashIDPreimageOperationID;
use Soneso\StellarSDK\Xdr\XdrLiquidityPoolConstantProductParameters;
use Soneso\StellarSDK\Xdr\XdrLiquidityPoolParameters;
use Soneso\StellarSDK\Xdr\XdrLiquidityPoolType;
use Soneso\StellarSDK\Xdr\XdrOperationType;
use Soneso\StellarSDK\Xdr\XdrSCMapEntry;
use Soneso\StellarSDK\Xdr\XdrSCVal;
use Soneso\StellarSDK\Xdr\XdrSequenceNumber;
use Soneso\StellarSDK\Xdr\XdrSignedPayload;
use Throwable;

final class BadRequest extends \Exception {}

final class Unsupported extends \Exception {}

/** A refusal the testee reports for the sdk, with the sdk's own words where there are some. */
final class Refused extends \Exception {}

/** An XdrBuffer that tells how much of its input the decoder read. */
final class CountingBuffer extends XdrBuffer
{
    public function left(): int
    {
        return strlen($this->xdrBytes) - $this->position;
    }
}

final class Ops
{
    public const PROTOCOL = 2;

    public const OPS = [
        'decode_encode', 'decode_json', 'encode_json', 'strkey_decode', 'strkey_encode',
        'asset_order', 'pool_id', 'map_sort', 'id_parse', 'to_native', 'address_from_scval',
        'tx_hash', 'balance_id', 'contract_id',
    ];

    // requests carry sdk json (encode_json) nested as deep as the xdr allows
    private const JSON_DEPTH = 1 << 20;

    /** The id of the request being answered, for the fatal error handler of bin/testee.php. */
    public static ?string $inFlight = null;

    /**
     * An engine \Error (a type error, a typed property read before it was set, an undefined
     * method, a ValueError from a php function the sdk handed a bad argument, a division by
     * zero) is the sdk crashing. Every \Exception is the sdk's refusal: it throws
     * InvalidArgumentException, RuntimeException and plain exceptions on purpose for bad input.
     * Running out of memory is a php fatal error that no catch sees; bin/testee.php answers it.
     */
    public static function isCrash(Throwable $e): bool
    {
        return $e instanceof \Error;
    }

    public static function handleLine(string $line): array
    {
        try {
            $req = json_decode($line, true, self::JSON_DEPTH, JSON_THROW_ON_ERROR | JSON_BIGINT_AS_STRING);
        } catch (\JsonException $e) {
            return self::error('', 'bad-request', 'request is not json: ' . $e->getMessage());
        }
        if (!is_array($req) || !isset($req['id']) || !is_string($req['id'])) {
            return self::error('', 'bad-request', 'request is not an object with a string id');
        }
        $id = $req['id'];
        self::$inFlight = $id;
        try {
            $op = $req['op'] ?? null;
            if (!is_string($op)) {
                throw new BadRequest('request has no op');
            }
            return ['id' => $id, 'status' => 'ok', 'result' => self::run($op, $req, $line)];
        } catch (BadRequest $e) {
            return self::error($id, 'bad-request', $e->getMessage());
        } catch (Unsupported $e) {
            return ['id' => $id, 'status' => 'unsupported', 'reason' => $e->getMessage()];
        } catch (Refused $e) {
            return self::error($id, 'rejected', $e->getMessage());
        } catch (Throwable $e) {
            // php names the file of a call in a TypeError; the build directory is this
            // machine's, so it is cut to the path inside the testee
            $message = str_replace(dirname(__DIR__) . '/', '', get_class($e) . ': ' . $e->getMessage());
            return self::error($id, self::isCrash($e) ? 'panic' : 'rejected', $message);
        } finally {
            self::$inFlight = null;
        }
    }

    private static function error(string $id, string $class, string $message): array
    {
        return ['id' => $id, 'status' => 'error', 'error' => ['class' => $class, 'message' => $message]];
    }

    private static function run(string $op, array $req, string $line): array
    {
        switch ($op) {
            case 'hello':
                return [
                    'protocol' => self::PROTOCOL,
                    'sdk' => 'soneso/stellar-php-sdk',
                    // the version composer installed, from its own record of the install
                    'sdk_version' => InstalledVersions::getPrettyVersion('soneso/stellar-php-sdk'),
                    'components' => (object) ['php' => PHP_VERSION],
                    'ops' => self::OPS,
                    // XdrBuffer takes no limits: it reads under its own DECODE_DEPTH_LIMIT
                    'decode_limits' => 'ignored',
                ];
            case 'decode_encode':
                $class = self::type($req);
                return ['xdr' => base64_encode(self::decode($class, self::bytes($req, 'xdr'))->encode())];
            case 'decode_json':
                $class = self::type($req);
                // decoded as objects, so an empty json object stays an object in the response
                $json = self::decode($class, self::bytes($req, 'xdr'))->toJson();
                return ['json' => json_decode($json, false, self::JSON_DEPTH, JSON_THROW_ON_ERROR)];
            case 'encode_json':
                $class = self::type($req);
                // the sdk's reader takes json text: the member is read as objects and written
                // back, so an empty object stays an object
                $tree = json_decode($line, false, self::JSON_DEPTH, JSON_THROW_ON_ERROR);
                if (!property_exists($tree, 'json')) {
                    throw new BadRequest('encode_json needs member json');
                }
                $json = json_encode($tree->json, JSON_THROW_ON_ERROR | JSON_UNESCAPED_SLASHES | JSON_PRESERVE_ZERO_FRACTION, self::JSON_DEPTH);
                return ['xdr' => base64_encode($class::fromJson($json)->encode())];
            case 'strkey_decode':
                return self::strkeyDecode($req);
            case 'strkey_encode':
                return self::strkeyEncode($req);
            case 'asset_order':
                return ['order' => self::assetOrder(self::asset($req, 'a'), self::asset($req, 'b'))];
            case 'pool_id':
                return self::poolId($req);
            case 'map_sort':
                return self::mapSort($req);
            case 'id_parse':
                return self::idParse($req);
            case 'to_native':
                return self::toNative($req);
            case 'address_from_scval':
                $val = self::decode(XdrSCVal::class, self::bytes($req, 'xdr'));
                return ['text' => Address::fromXdrSCVal($val)->toStrKey()];
            case 'tx_hash':
                return self::txHash($req);
            case 'balance_id':
                return self::balanceId($req);
            case 'contract_id':
                return self::contractId($req);
            case 'map_validate':
                throw new Unsupported('soneso/stellar-php-sdk has no ScMap validation: it orders keys the host way '
                    . '(ScValHostOrder::compare, XdrSCVal::forMap) but offers no check of a map it is given');
            case 'coverage_snapshot':
            case 'coverage_new':
                throw new Unsupported('the php testee has no coverage build');
            default:
                throw new BadRequest("unknown op $op");
        }
    }

    private static function string(array $req, string $name): string
    {
        if (!isset($req[$name]) || !is_string($req[$name])) {
            throw new BadRequest("$name must be a string");
        }
        return $req[$name];
    }

    private static function bytes(array $req, string $name): string
    {
        $text = self::string($req, $name);
        // strict base64_decode still takes input without its padding, which the protocol refuses
        $b = strlen($text) % 4 === 0 ? base64_decode($text, true) : false;
        if ($b === false) {
            throw new BadRequest("$name is not base64");
        }
        return $b;
    }

    /** The sdk class of the request's type, by its name without regard to case. */
    private static function type(array $req): string
    {
        $key = strtolower(self::string($req, 'type'));
        if (isset(TablesGen::TYPES[$key])) {
            return TablesGen::TYPES[$key];
        }
        throw new Unsupported(TablesGen::UNMAPPED[$key]
            ?? 'soneso/stellar-php-sdk ' . StellarSDK::VERSION_NR . ' has no class for the xdr type ' . $req['type']);
    }

    /** Decodes the whole input: bytes left after the value are a refusal, as the protocol requires. */
    private static function decode(string $class, string $bytes): object
    {
        $buffer = new CountingBuffer($bytes);
        $value = $class::decode($buffer);
        $left = $buffer->left();
        if ($left !== 0) {
            throw new Refused("$left trailing bytes after the value");
        }
        return $value;
    }

    private static function strkeyDecode(array $req): array
    {
        if (array_key_exists('text', $req) && !array_key_exists('text_base64', $req)) {
            $text = self::string($req, 'text');
        } elseif (array_key_exists('text_base64', $req) && !array_key_exists('text', $req)) {
            // php strings are bytes, so text that is not utf-8 reaches the decoders as it is
            $text = self::bytes($req, 'text_base64');
        } else {
            throw new BadRequest('strkey_decode needs exactly one of text and text_base64');
        }
        if ($text === '') {
            throw new Refused('empty strkey');
        }
        switch ($text[0]) {
            case 'G': return self::kind('account', StrKey::decodeAccountId($text));
            case 'M': return self::kind('muxed_account', StrKey::decodeMuxedAccountId($text));
            case 'S': return self::kind('seed', StrKey::decodeSeed($text));
            case 'T': return self::kind('pre_auth_tx', StrKey::decodePreAuthTx($text));
            case 'X': return self::kind('hash_x', StrKey::decodeSha256Hash($text));
            case 'C': return self::kind('contract', StrKey::decodeContractId($text));
            case 'L': return self::kind('liquidity_pool', StrKey::decodeLiquidityPoolId($text));
            case 'B': return self::kind('claimable_balance', StrKey::decodeClaimableBalanceId($text));
            case 'W': return self::kind('muxed_contract', StrKey::decodeMuxedContractId($text));
            case 'P':
                $p = StrKey::decodeXdrSignedPayload($text);
                $n = strlen($p->payload);
                $raw = $p->ed25519 . pack('N', $n) . $p->payload . str_repeat("\0", (4 - $n % 4) % 4);
                return self::kind('signed_payload', $raw);
            default:
                throw new Refused('no strkey decoder for a string starting with ' . $text[0]);
        }
    }

    private static function kind(string $kind, string $payload): array
    {
        return ['kind' => $kind, 'payload' => base64_encode($payload)];
    }

    private static function strkeyEncode(array $req): array
    {
        $kind = self::string($req, 'kind');
        $p = self::bytes($req, 'payload');
        switch ($kind) {
            case 'account': $t = StrKey::encodeAccountId($p); break;
            case 'muxed_account': $t = StrKey::encodeMuxedAccountId($p); break;
            case 'seed': $t = StrKey::encodeSeed($p); break;
            case 'pre_auth_tx': $t = StrKey::encodePreAuthTx($p); break;
            case 'hash_x': $t = StrKey::encodeSha256Hash($p); break;
            case 'contract': $t = StrKey::encodeContractId($p); break;
            case 'liquidity_pool': $t = StrKey::encodeLiquidityPoolId($p); break;
            case 'claimable_balance': $t = StrKey::encodeClaimableBalanceId($p); break;
            case 'muxed_contract': $t = StrKey::encodeMuxedContractId($p); break;
            case 'signed_payload':
                // the encoder takes an XdrSignedPayload: the key, then the payload its length names
                if (strlen($p) < 36) {
                    throw new Refused('a signed payload needs a key and a length');
                }
                $n = unpack('N', substr($p, 32, 4))[1];
                if (36 + $n > strlen($p)) {
                    throw new Refused('the payload length runs past the input');
                }
                $padded = 36 + $n + (4 - $n % 4) % 4;
                if (strlen($p) !== $padded || substr($p, 36 + $n) !== str_repeat("\0", $padded - 36 - $n)) {
                    throw new Unsupported('StrKey::encodeXdrSignedPayload takes the key and the payload; a raw payload '
                        . 'with padding that is not zero or bytes after it has no such form');
                }
                $t = StrKey::encodeXdrSignedPayload(new XdrSignedPayload(substr($p, 0, 32), substr($p, 36, $n)));
                break;
            default:
                throw new BadRequest("unknown strkey kind $kind");
        }
        return ['text' => $t];
    }

    private static function asset(array $req, string $name): Asset
    {
        return Asset::fromXdr(self::decode(TablesGen::TYPES['asset'], self::bytes($req, $name)));
    }

    private static function orders(Asset $a, Asset $b): bool
    {
        try {
            new AssetTypePoolShare($a, $b);
            return true;
        } catch (Throwable $e) {
            if (self::isCrash($e)) {
                throw $e;
            }
            return false;
        }
    }

    /**
     * The sdk's only asset ordering is the check in AssetTypePoolShare's constructor, which
     * refuses an assetA that sorts after assetB (Soneso/StellarSDK/AssetTypePoolShare.php). Two
     * native assets it refuses in either order, so they have no answer.
     */
    private static function assetOrder(Asset $a, Asset $b): int
    {
        $ab = self::orders($a, $b);
        $ba = self::orders($b, $a);
        if ($ab && $ba) {
            return 0;
        }
        if ($ab) {
            return -1;
        }
        if ($ba) {
            return 1;
        }
        throw new Refused('AssetTypePoolShare refuses both orders of these assets');
    }

    /**
     * The sdk has no pool id function. As the protocol says for that case, the testee hashes the
     * LiquidityPoolParameters the sdk's own encoder writes, for pairs the sdk's own ordering puts
     * strictly in order.
     */
    private static function poolId(array $req): array
    {
        $a = self::asset($req, 'a');
        $b = self::asset($req, 'b');
        $fee = $req['fee'] ?? null;
        if (!is_int($fee)) {
            throw new BadRequest('fee must be an integer');
        }
        new AssetTypePoolShare($a, $b);
        if (self::orders($b, $a)) {
            throw new Refused('the assets are equal');
        }
        $params = new XdrLiquidityPoolParameters(new XdrLiquidityPoolType(XdrLiquidityPoolType::LIQUIDITY_POOL_CONSTANT_PRODUCT));
        $params->constantProduct = new XdrLiquidityPoolConstantProductParameters($a->toXdr(), $b->toXdr(), $fee);
        return ['pool_id' => hash('sha256', $params->encode())];
    }

    private static function mapSort(array $req): array
    {
        $entries = $req['entries'] ?? null;
        if (!is_array($entries)) {
            throw new BadRequest('entries must be an array');
        }
        $map = [];
        foreach ($entries as $e) {
            if (!is_array($e)) {
                throw new BadRequest('an entry is not an object');
            }
            $map[] = new XdrSCMapEntry(
                self::decode(XdrSCVal::class, self::bytes($e, 'key')),
                self::decode(XdrSCVal::class, self::bytes($e, 'val')),
            );
        }
        return ['xdr' => base64_encode(XdrSCVal::forMap($map)->encode())];
    }

    /**
     * ManageSellOfferOperationBuilder::setOfferId and MuxedAccount take the id as a php int, so
     * a json number within the int range is handed over as that int; text and numbers outside
     * the range have no int form.
     */
    private static function idParse(array $req): array
    {
        $kind = self::string($req, 'kind');
        $input = $req['input'] ?? null;
        if (!is_int($input)) {
            throw new Unsupported('soneso/stellar-php-sdk takes ' . $kind . ' ids as a php int only '
                . '(ManageSellOfferOperationBuilder::setOfferId, MuxedAccount::__construct); this input has no int form');
        }
        $issuer = 'GD7777777777777777777777777777777777777777777777777773DB';
        if ($kind === 'offer') {
            $op = (new ManageSellOfferOperationBuilder(Asset::native(), new AssetTypeCreditAlphanum4('ABC', $issuer), '1', '1'))
                ->setOfferId($input)->build();
            $bytes = $op->toOperationBody()->manageSellOfferOp->encode();
            // offerID is the op's last field (ManageSellOfferOp in Stellar-transaction.x)
            return ['id' => (string) unpack('J', substr($bytes, -8))[1]];
        }
        if ($kind === 'muxed') {
            $bytes = (new MuxedAccount($issuer, $input))->toXdr()->encode();
            // KEY_TYPE_MUXED_ED25519: the type, then the uint64 id, then the key
            $id = unpack('J', substr($bytes, 4, 8))[1];
            return ['id' => $id >= 0 ? (string) $id : sprintf('%u', $id)];
        }
        throw new BadRequest("unknown id kind $kind");
    }

    private static function toNative(array $req): array
    {
        $val = self::decode(XdrSCVal::class, self::bytes($req, 'xdr'));
        $native = $val->toNative();
        // the documented refusal: a value with no native form comes back as the XdrSCVal itself
        if ($native instanceof XdrSCVal) {
            throw new Refused('toNative returned the XdrSCVal itself: no native form');
        }
        return ['native' => self::describe($native)];
    }

    private static function describe(mixed $v): array
    {
        if ($v === null) {
            return ['type' => 'null'];
        }
        if (is_bool($v)) {
            return ['type' => 'bool', 'value' => $v];
        }
        if (is_int($v)) {
            return ['type' => 'integer', 'value' => (string) $v];
        }
        if ($v instanceof \GMP) {
            return ['type' => 'integer', 'value' => gmp_strval($v)];
        }
        if ($v instanceof \phpseclib3\Math\BigInteger) {
            return ['type' => 'integer', 'value' => $v->toString()];
        }
        if (is_string($v)) {
            // php has one string type for text and bytes; the sdk hands both out as strings
            return mb_check_encoding($v, 'UTF-8')
                ? ['type' => 'string', 'value' => $v]
                : ['type' => 'bytes', 'value' => base64_encode($v)];
        }
        if (is_array($v)) {
            if (array_is_list($v)) {
                return ['type' => 'array', 'items' => array_map([self::class, 'describe'], $v)];
            }
            $entries = [];
            foreach ($v as $k => $x) {
                $entries[] = ['key' => self::describe($k), 'value' => self::describe($x)];
            }
            return ['type' => 'map', 'entries' => $entries];
        }
        if ($v instanceof Address) {
            return ['type' => 'object', 'class' => Address::class, 'text' => $v->toStrKey()];
        }
        if ($v instanceof XdrSCVal) {
            return ['type' => 'object', 'class' => XdrSCVal::class, 'text' => base64_encode($v->encode())];
        }
        return ['type' => 'object', 'class' => get_debug_type($v), 'text' => method_exists($v, '__toString') ? (string) $v : get_debug_type($v)];
    }

    private static function network(array $req): Network
    {
        return new Network(self::string($req, 'network_passphrase'));
    }

    private static function transaction(array $req): AbstractTransaction
    {
        return AbstractTransaction::fromEnvelopeXdr(self::decode(TablesGen::TYPES['transactionenvelope'], self::bytes($req, 'xdr')));
    }

    private static function txHash(array $req): array
    {
        $tx = self::transaction($req);
        $network = self::network($req);
        $inner = $req['inner'] ?? false;
        if (!is_bool($inner)) {
            throw new BadRequest('inner must be a boolean');
        }
        if ($inner) {
            if (!$tx instanceof FeeBumpTransaction) {
                throw new BadRequest('inner on an envelope that is no fee bump');
            }
            return ['hash' => bin2hex($tx->getInnerTx()->hash($network))];
        }
        return ['hash' => bin2hex($tx->hash($network))];
    }

    /**
     * The sdk has no balance id function, so the testee hashes the HashIDPreimage of the
     * operation (ENVELOPE_TYPE_OP_ID) that the sdk's own encoder writes, from the sdk's
     * transaction object.
     */
    private static function balanceId(array $req): array
    {
        $tx = self::transaction($req);
        if ($tx instanceof FeeBumpTransaction) {
            $tx = $tx->getInnerTx();
        }
        if (!$tx instanceof Transaction) {
            throw new BadRequest('the envelope holds no transaction');
        }
        $index = $req['op_index'] ?? null;
        if (!is_int($index) || $index < 0) {
            throw new BadRequest('op_index must be a non-negative integer');
        }
        $ops = $tx->getOperations();
        if ($index >= count($ops)) {
            throw new Refused("no operation at index $index");
        }
        $body = $ops[$index]->toOperationBody();
        if ($body->type->value !== XdrOperationType::CREATE_CLAIMABLE_BALANCE) {
            throw new Refused("the operation at index $index does not create a claimable balance");
        }
        $source = KeyPair::fromAccountId($tx->getSourceAccount()->getEd25519AccountId());
        $preimage = new XdrHashIDPreimage(new XdrEnvelopeType(XdrEnvelopeType::ENVELOPE_TYPE_OP_ID));
        $preimage->operationID = new XdrHashIDPreimageOperationID(
            new XdrAccountID($source->getAccountId()),
            new XdrSequenceNumber($tx->getSequenceNumber()),
            $index,
        );
        return ['balance_id' => '00000000' . hash('sha256', $preimage->encode())];
    }

    /**
     * Address::deriveContractId for address preimages; the sdk has no function for asset
     * preimages, so for those the testee hashes the HashIDPreimage its encoder writes, the way
     * deriveContractId does.
     */
    private static function contractId(array $req): array
    {
        $preimage = self::decode(TablesGen::TYPES['contractidpreimage'], self::bytes($req, 'preimage'));
        $network = self::network($req);
        if ($preimage->type->value === XdrContractIDPreimageType::CONTRACT_ID_PREIMAGE_FROM_ADDRESS) {
            $id = Address::deriveContractId(Address::fromXdr($preimage->address), $preimage->salt, $network);
            return ['contract_id' => bin2hex(StrKey::decodeContractId($id))];
        }
        $hp = new XdrHashIDPreimage(new XdrEnvelopeType(XdrEnvelopeType::ENVELOPE_TYPE_CONTRACT_ID));
        $hp->contractID = new XdrHashIDPreimageContractID(hash('sha256', $network->getNetworkPassphrase(), true), $preimage);
        return ['contract_id' => hash('sha256', $hp->encode())];
    }
}
