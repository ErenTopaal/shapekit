<?php declare(strict_types=1);

// writes src/TablesGen.php for the php testee: the soneso/stellar-php-sdk class behind every
// type the xdr schema defines (schema/xdr/<tag>/xdr.json), read from the installed sdk.
//
// a schema type matches the class Soneso\StellarSDK\Xdr\Xdr<name> whose name equals it
// without regard to case (SCVal and ScVal are XdrSCVal). where the sdk names a type another
// way, the class is listed in ALIASES below with the reason. the class must have decode,
// encode, toJson and fromJson, its own or inherited. schema types without such a class are
// written to UNMAPPED, so the testee answers unsupported for them with the reason.
//
// usage, from the testee directory after composer install:
//   php tools/gentypes.php <schema xdr.json> <out file>

require __DIR__ . '/../vendor/autoload.php';

use Soneso\StellarSDK\StellarSDK;

// schema name (lower case) => sdk class, where the sdk's name differs. each was checked to be
// the same xdr shape and the same SEP-51 rendering in the sdk source.
const ALIASES = [
    // Xdr/XdrAccountEntryV1.php and its neighbours: the sdk drops "Extension"
    'accountentryextensionv1' => 'XdrAccountEntryV1',
    'accountentryextensionv1ext' => 'XdrAccountEntryV1Ext',
    'accountentryextensionv2' => 'XdrAccountEntryV2',
    'accountentryextensionv2ext' => 'XdrAccountEntryV2Ext',
    'accountentryextensionv3' => 'XdrAccountEntryV3',
    'claimablebalanceentryextensionv1' => 'XdrClaimableBalanceEntryExtV1',
    'claimablebalanceentryextensionv1ext' => 'XdrClaimableBalanceEntryExtV1Ext',
    // Xdr/XdrAssetAlphaNum4.php, Xdr/XdrAssetAlphaNum12.php
    'alphanum4' => 'XdrAssetAlphaNum4',
    'alphanum12' => 'XdrAssetAlphaNum12',
    // Xdr/XdrManageOfferResult.php: one class for both offer operations, whose result unions
    // and codes have the same values and, prefix stripped, the same SEP-51 names
    'managesellofferresult' => 'XdrManageOfferResult',
    'managebuyofferresult' => 'XdrManageOfferResult',
    'managesellofferresultcode' => 'XdrManageOfferResultCode',
    'managebuyofferresultcode' => 'XdrManageOfferResultCode',
    // Xdr/XdrPathPaymentResultSuccess.php: one class for both path payments
    'pathpaymentstrictreceiveresultsuccess' => 'XdrPathPaymentResultSuccess',
    'pathpaymentstrictsendresultsuccess' => 'XdrPathPaymentResultSuccess',
    // the operations: the sdk names each <Name>Operation where the xdr says <Name>Op
    'createaccountop' => 'XdrCreateAccountOperation',
    'paymentop' => 'XdrPaymentOperation',
    'pathpaymentstrictreceiveop' => 'XdrPathPaymentStrictReceiveOperation',
    'pathpaymentstrictsendop' => 'XdrPathPaymentStrictSendOperation',
    'managesellofferop' => 'XdrManageSellOfferOperation',
    'managebuyofferop' => 'XdrManageBuyOfferOperation',
    'createpassivesellofferop' => 'XdrCreatePassiveSellOfferOperation',
    'setoptionsop' => 'XdrSetOptionsOperation',
    'changetrustop' => 'XdrChangeTrustOperation',
    'allowtrustop' => 'XdrAllowTrustOperation',
    'managedataop' => 'XdrManageDataOperation',
    'bumpsequenceop' => 'XdrBumpSequenceOperation',
    'createclaimablebalanceop' => 'XdrCreateClaimableBalanceOperation',
    'claimclaimablebalanceop' => 'XdrClaimClaimableBalanceOperation',
    'beginsponsoringfuturereservesop' => 'XdrBeginSponsoringFutureReservesOperation',
    'revokesponsorshipop' => 'XdrRevokeSponsorshipOperation',
    'clawbackop' => 'XdrClawbackOperation',
    'clawbackclaimablebalanceop' => 'XdrClawbackClaimableBalanceOperation',
    'settrustlineflagsop' => 'XdrSetTrustLineFlagsOperation',
    'liquiditypooldepositop' => 'XdrLiquidityPoolDepositOperation',
    'liquiditypoolwithdrawop' => 'XdrLiquidityPoolWithdrawOperation',
    // Xdr/XdrRevokeSponsorshipSigner.php
    'revokesponsorshipopsigner' => 'XdrRevokeSponsorshipSigner',
    // Xdr/XdrSignedPayload.php
    'signerkeyed25519signedpayload' => 'XdrSignedPayload',
    // Xdr/XdrContractEventBodyV0.php
    'contracteventv0' => 'XdrContractEventBodyV0',
    // the overlay's Error, which stellar-xdr names SError; the request carries SError
    'serror' => 'XdrError',
];

if ($argc !== 3) {
    fwrite(STDERR, "usage: php tools/gentypes.php <schema xdr.json> <out file>\n");
    exit(2);
}
$schema = json_decode(file_get_contents($argv[1]), true, 512, JSON_THROW_ON_ERROR);
$names = [];
foreach ($schema['definitions'] as $d) {
    if (in_array($d['kind'], ['struct', 'union', 'enum', 'typedef'], true)) {
        $names[] = $d['name'];
    }
}
if ($names === []) {
    throw new RuntimeException("$argv[1] defines no type");
}
sort($names);

$dir = (new ReflectionClass(StellarSDK::class))->getFileName();
$xdrDir = dirname($dir) . '/Xdr';
$classes = [];
foreach (scandir($xdrDir) as $f) {
    if (preg_match('/^(Xdr\w+)\.php$/', $f, $m) === 1) {
        $key = strtolower(substr($m[1], 3));
        if (isset($classes[$key])) {
            throw new RuntimeException("$key names {$classes[$key]} and {$m[1]}");
        }
        $classes[$key] = $m[1];
    }
}

$version = StellarSDK::VERSION_NR;
$mapped = [];
$unmapped = [];
$usable = static function (string $fqcn): array {
    $missing = [];
    foreach (['decode', 'fromJson'] as $m) {
        if (!method_exists($fqcn, $m) || !(new ReflectionMethod($fqcn, $m))->isStatic()) {
            $missing[] = "static $m";
        }
    }
    foreach (['encode', 'toJson'] as $m) {
        if (!method_exists($fqcn, $m)) {
            $missing[] = $m;
        }
    }
    return $missing;
};
$wanted = array_map('strtolower', $names);
$wanted[] = 'serror';
foreach (array_unique($wanted) as $key) {
    $name = $names[array_search($key, array_map('strtolower', $names), true)] ?? 'SError';
    $cls = ALIASES[$key] ?? ($classes[$key] ?? null);
    if ($cls === null || $cls === '') {
        $unmapped[$key] = "soneso/stellar-php-sdk $version has no class for the xdr type $name";
        continue;
    }
    $fqcn = 'Soneso\\StellarSDK\\Xdr\\' . $cls;
    if (!class_exists($fqcn)) {
        throw new RuntimeException("alias $key names $fqcn, which does not exist");
    }
    $missing = $usable($fqcn);
    if ($missing !== []) {
        $unmapped[$key] = "soneso/stellar-php-sdk $version class $cls for $name has no " . implode(', ', $missing);
        continue;
    }
    $mapped[$key] = $fqcn;
}
foreach (array_keys(ALIASES) as $a) {
    if (ALIASES[$a] !== '' && $a !== 'serror' && !in_array($a, $wanted, true)) {
        throw new RuntimeException("alias $a names no schema type");
    }
}
ksort($mapped);
ksort($unmapped);

$schemaName = implode('/', array_slice(explode('/', $argv[1]), -2));
$out = "<?php declare(strict_types=1);\n\n// Code generated by tools/gentypes.php from $schemaName and\n// soneso/stellar-php-sdk $version. DO NOT EDIT.\n\nnamespace Shapekit\\Testee;\n\nfinal class TablesGen\n{\n";
$out .= "    /** Every xdr type with an sdk class, by its name in lower case. */\n    public const TYPES = [\n";
foreach ($mapped as $k => $c) {
    $out .= '        ' . var_export($k, true) . ' => \\' . $c . "::class,\n";
}
$out .= "    ];\n\n    /** Why the schema types without a usable class have none, by name in lower case. */\n    public const UNMAPPED = [\n";
foreach ($unmapped as $k => $why) {
    $out .= '        ' . var_export($k, true) . ' => ' . var_export($why, true) . ",\n";
}
$out .= "    ];\n}\n";
file_put_contents($argv[2], $out);
printf("%d types mapped, %d without a class\n", count($mapped), count($unmapped));
