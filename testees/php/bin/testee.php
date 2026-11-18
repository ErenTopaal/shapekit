<?php declare(strict_types=1);

// the php testee for the shapekit conformance runner: answers testee protocol requests
// (docs/testee-protocol.md) with soneso/stellar-php-sdk. one source, installed once per
// pinned sdk version from the composer files under versions/.

require __DIR__ . '/../vendor/autoload.php';

use Shapekit\Testee\Ops;

// php keeps the frames of a php call on the heap, so a deep recursion in the sdk grows memory
// rather than the stack. a finite limit below the runner's 4096 MiB rlimit
// (conformance/versions.toml) lets php stop that request itself, and the shutdown function below
// answers it as the crash it is; the runner then restarts the testee
ini_set('memory_limit', '3072M');
// stdout carries responses only; php writes warnings and notices to stdout by default
ini_set('display_errors', 'stderr');
register_shutdown_function(static function (): void {
    $fatal = error_get_last();
    $id = Ops::$inFlight;
    if ($id === null || $fatal === null || !in_array($fatal['type'], [E_ERROR, E_CORE_ERROR, E_COMPILE_ERROR, E_USER_ERROR], true)) {
        return;
    }
    fwrite(STDOUT, json_encode([
        'id' => $id,
        'status' => 'error',
        'error' => ['class' => 'panic', 'message' => 'php fatal error: ' . $fatal['message']],
    ], JSON_INVALID_UTF8_SUBSTITUTE | JSON_UNESCAPED_SLASHES) . "\n");
    fflush(STDOUT);
});
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
        if ($response['status'] === 'error') {
            // an sdk message that is not utf-8 keeps the answer's class; only its text changes
            $out = json_encode($response, JSON_THROW_ON_ERROR | JSON_UNESCAPED_SLASHES | JSON_INVALID_UTF8_SUBSTITUTE);
        } else {
            // a result with no json form is the testee failing to answer, not the sdk's verdict
            $out = json_encode([
                'id' => $response['id'],
                'status' => 'error',
                'error' => ['class' => 'internal', 'message' => 'the response has no json form: ' . $e->getMessage()],
            ], JSON_THROW_ON_ERROR | JSON_INVALID_UTF8_SUBSTITUTE);
        }
    }
    fwrite(STDOUT, $out . "\n");
    fflush(STDOUT);
}
