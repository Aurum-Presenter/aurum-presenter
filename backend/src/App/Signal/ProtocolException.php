<?php

declare(strict_types=1);

namespace App\Signal;

/** Something the relay refuses to speak. The socket is closed rather than answered. */
final class ProtocolException extends \RuntimeException
{
}
