<?php declare(strict_types=1);

// unit checks of the php testee, run as `php tests/run.php` from the build directory

require __DIR__ . '/../vendor/autoload.php';

use Shapekit\Testee\Ops;

$failed = 0;
function check(string $name, array $request, callable $ok): void
{
    global $failed;
    $r = Ops::handleLine(json_encode($request));
    if ($ok($r)) {
        echo "ok   $name\n";
    } else {
        echo "FAIL $name: " . json_encode($r) . "\n";
        $failed++;
    }
}

check('hello names the installed sdk', ['id' => '1', 'op' => 'hello'],
    fn($r) => $r['status'] === 'ok' && $r['result']['sdk'] === 'soneso/stellar-php-sdk' && $r['result']['sdk_version'] !== '');
check('an ScVal round trips', ['id' => '2', 'op' => 'decode_encode', 'type' => 'ScVal', 'xdr' => 'AAAAAwAAAAc=', 'depth_limit' => 1000, 'len_limit' => 8],
    fn($r) => ($r['result']['xdr'] ?? null) === 'AAAAAwAAAAc=');
check('trailing bytes are a refusal', ['id' => '3', 'op' => 'decode_encode', 'type' => 'ScVal', 'xdr' => 'AAAAAwAAAAcA', 'depth_limit' => 1000, 'len_limit' => 9],
    fn($r) => ($r['error']['class'] ?? null) === 'rejected');
check('type names match without regard to case', ['id' => '4', 'op' => 'decode_json', 'type' => 'SCVal', 'xdr' => 'AAAAAwAAAAc=', 'depth_limit' => 1000, 'len_limit' => 8],
    fn($r) => json_encode($r['result']['json'] ?? null) === '{"u32":7}');
check('encode_json reads the json member', ['id' => '5', 'op' => 'encode_json', 'type' => 'ScVal', 'json' => ['u32' => 7]],
    fn($r) => ($r['result']['xdr'] ?? null) === 'AAAAAwAAAAc=');
$g = 'GD7777777777777777777777777777777777777777777777777773DB';
check('an account strkey decodes', ['id' => '6', 'op' => 'strkey_decode', 'text' => $g],
    fn($r) => ($r['result']['kind'] ?? null) === 'account');
check('a type the sdk lacks is unsupported', ['id' => '7', 'op' => 'decode_encode', 'type' => 'ScSymbol', 'xdr' => 'AAAAAA==', 'depth_limit' => 1000, 'len_limit' => 4],
    fn($r) => $r['status'] === 'unsupported');
check('base64 without its padding is a bad request', ['id' => '8', 'op' => 'decode_encode', 'type' => 'ScVal', 'xdr' => 'AAAAAwAAAAc', 'depth_limit' => 1000, 'len_limit' => 8],
    fn($r) => ($r['error']['class'] ?? null) === 'bad-request');
$crash = Ops::isCrash(new \Error('Typed property X::$y must not be accessed before initialization'))
    && Ops::isCrash(new \ValueError('x')) && !Ops::isCrash(new \InvalidArgumentException('x'));
echo ($crash ? 'ok  ' : 'FAIL') . " an engine error is a crash, an exception a refusal\n";
$failed += $crash ? 0 : 1;
exit($failed === 0 ? 0 : 1);
