<?php

declare(strict_types=1);

namespace App\Auth;

use RuntimeException;
use SensitiveParameter;

/**
 * AES-256-GCM for the TOTP shared secret. Authenticated encryption matters here: a tampered
 * secret must fail loudly rather than silently start accepting a different authenticator's
 * codes.
 */
final class SecretCipher
{
    private const string CIPHER = 'aes-256-gcm';

    private string $key;

    public function __construct(#[SensitiveParameter] string $base64Key)
    {
        $key = base64_decode($base64Key, true);

        if ($key === false || strlen($key) !== 32) {
            throw new RuntimeException(
                'APP_SECRET_KEY must be 32 bytes, base64-encoded. Generate one with: '
                . 'openssl rand -base64 32'
            );
        }

        $this->key = $key;
    }

    public function encrypt(#[SensitiveParameter] string $plaintext): string
    {
        $iv = random_bytes(12);
        $tag = '';

        $ciphertext = openssl_encrypt($plaintext, self::CIPHER, $this->key, OPENSSL_RAW_DATA, $iv, $tag);
        if ($ciphertext === false) {
            throw new RuntimeException('Unable to encrypt secret.');
        }

        return $iv . $tag . $ciphertext;
    }

    public function decrypt(string $payload): string
    {
        if (strlen($payload) < 29) {
            throw new RuntimeException('Encrypted secret is truncated.');
        }

        $iv = substr($payload, 0, 12);
        $tag = substr($payload, 12, 16);
        $ciphertext = substr($payload, 28);

        $plaintext = openssl_decrypt($ciphertext, self::CIPHER, $this->key, OPENSSL_RAW_DATA, $iv, $tag);
        if ($plaintext === false) {
            throw new RuntimeException('Unable to decrypt secret: wrong key or tampered payload.');
        }

        return $plaintext;
    }
}
