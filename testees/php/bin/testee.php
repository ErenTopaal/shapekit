<?php declare(strict_types=1);

// the php testee for the shapekit conformance runner: answers testee protocol requests
// (docs/testee-protocol.md) with soneso/stellar-php-sdk. one source, installed once per
// pinned sdk version from the composer files under versions/.

require __DIR__ . '/../vendor/autoload.php';

use Shapekit\Testee\Ops;

ini_set('memory_limit', '-1');
// stdout carries responses only; php writes warnings and notices to stdout by default
ini_set('display_errors', 'stderr');
$in = fopen('php://stdin', 'rb');
while (($line = fgets($in)) !== false) {
    $line = rtrim($line, "\r\n");
    if ($line === '') {
        continue;
    }
    $response = Ops::handleLine($line);
    try {
        $out = json_encode($response, JSON_THROW_ON_ERROR | JSON_UNESCAPED_SLASHES | JSON_PRESERVE_ZERO_FRACTION);
    } catch (JsonException $e) {
        // a value with no json form (bytes that are not utf-8 in a message or result) is the
        // testee failing to answer, not the sdk's verdict
        $out = json_encode([
            'id' => $response['id'],
            'status' => 'error',
            'error' => ['class' => 'internal', 'message' => 'the response has no json form: ' . $e->getMessage()],
        ], JSON_THROW_ON_ERROR | JSON_INVALID_UTF8_SUBSTITUTE);
    }
    fwrite(STDOUT, $out . "\n");
    fflush(STDOUT);
}
