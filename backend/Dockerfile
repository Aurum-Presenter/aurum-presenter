# FrankenPHP serves the API. It is also the runtime that can hold a WebSocket open, which the
# stage-view signalling change request needs and Apache + PHP-FPM cannot do — see that CR's
# open question before committing to it for production.
FROM dunglas/frankenphp:1-php8.4

COPY --from=composer/composer:2-bin /composer /usr/bin/composer

RUN install-php-extensions \
        pdo_sqlite \
        intl \
        opcache \
        zip \
    && apt-get update \
    && apt-get install -y --no-install-recommends sqlite3 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Dependencies are installed in their own layer so a source edit does not re-resolve them.
COPY composer.json composer.lock ./
RUN composer install --no-interaction --no-scripts --no-autoloader --prefer-dist

# Explicit rather than relying on the base image's default config. FrankenPHP's stock
# Caddyfile happens to serve /app/public, so omitting this works by coincidence — which is
# exactly the kind of thing that breaks silently on a base image bump.
COPY Caddyfile /etc/frankenphp/Caddyfile

COPY . .
RUN composer dump-autoload --optimize \
    && mkdir -p var/data/workspace data/cache \
    && chown -R www-data:www-data var data

ENV SERVER_NAME=":80"

EXPOSE 80
