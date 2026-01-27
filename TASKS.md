# claw-screenshot — Задача и техническое описание

Дата: 2026-01-27
Автор: Clawdbot (автоматическое изменение по просьбе Мак)

----------------------

## Краткая цель проекта

claw-screenshot — небольшой CLI на Rust, предназначенный для запроса снимка экрана у свободной среды desktop portal (Freedesktop Portals), получения результата (URI временного файла) и копирования конечного PNG в директорию пользователя `~/clawd/screenshots`. CLI используется из helper-скрипта (`screenshot-helper.sh`) и systemd --user сервиса для интеграции с системой (быстрый «снимок экрана» по хоткею/триггеру).

Проект должен:
- Вызывать org.freedesktop.portal.Screenshot via D-Bus (Portal API).
- Ожидать ответ (Request.Response) для конкретного запроса и извлечь возвращённый URI (обычно file://... временный файл).
- Скопировать файл в `~/clawd/screenshots` и вывести в stdout строку `SAVED:<path>` или подробный лог ошибки.
- Работать корректно в blocking (не-async) окружении, минимально зависеть от окружения пользователя, логировать детали для fallback-обработчика.

----------------------

## Текущее состояние

- Репозиторий `~/clawd/claw-screenshot` создан и содержит базовый prototype.
- Код реализует вызов `org.freedesktop.portal.Screenshot` через zbus (blocking) и пытается поймать сигнал `org.freedesktop.portal.Request.Response` для returned request object path.
- Добавлен robust парсер `extract_uri_from_map` для извлечения `uri` из возвращаемой структуры `a{sv}` (учитывает вариативность формата).
- В процессе: корректная реализация получения сигнала Response в blocking zbus v5 (в среде наблюдаются различия API и отсутствуют некоторые примеры/утилиты). В рабочем helper используется fallback через gdbus; цель — убрать fallback и полагаться на Rust CLI.

----------------------

## Техническая задача (подробно)

1) Вызвать метод Screenshot:
   - D-Bus service: `org.freedesktop.portal.Desktop`
   - Object path: `/org/freedesktop/portal/desktop`
   - Interface: `org.freedesktop.portal.Screenshot`
   - Method: `Screenshot` (аргументы: interactive? false, options {}

2) Метод возвращает объект-запрос (ObjectPath). Портал создаёт объект типа `org.freedesktop.portal.Request` по этому пути и позже эмитит сигнал `Response` на этом объекте с сигнатурой `(u, a{sv})` (u — код успеха/ошибки, a{sv} — результаты).

3) Задача — подписаться на сигнал `Response` именно на том object path (reply), распарсить a{sv} и извлечь поле `uri` (обычно строка `file:///tmp/snap-....png`), декодировать URL, проверить существование файла, скопировать его в `~/clawd/screenshots`.

4) Таймаут: ждать не дольше 10 секунд, затем аккуратно завершать с ошибкой (helper — fallback).

5) Логирование: подробный stderr (eprintln!) с diagnostics: request object path, код ответа, содержимое map (debug), финальные шаги копирования/ошибки.

6) Совместимость: использовать blocking API (по требований интеграции с helper), но при необходимости можно перейти на async-реализацию (с минимальным рефакторингом), если blocking API в zbus в целевой системе не позволяет удобно подписываться на одиночный сигнал.

----------------------

## Технические сложности и наблюдения

- zbus версии в целевой среде (v5.x) имеет отличия в blocking API от примеров из документации/интернета. В частности, некоторые высокоуровневые методы для получения потока сигналов (`receive_signal`, `receive_message`) могут отсутствовать.
- Возможные подходы для получения сигнала Response:
  - Создать `zbus::blocking::Proxy` для интерфейса `org.freedesktop.portal.Screenshot` и вызвать `call("Screenshot", ...)`, затем подписаться на сигнал `Response` через proxy, если API предоставляет такую возможность (Proxy::connect_signal/SignalHandler). Нужно проверить, поддерживает ли blocking Proxy подписку на сигналы в используемой версии.
  - Низкоуровневый подход: зарегистрировать D-Bus match rule (add_match) на Connection с фильтром по `interface=org.freedesktop.portal.Request` и `path=<reply>`; затем читать входящие сообщения через низкоуровневый iterator/метод и фильтровать по member == "Response".
  - Переключиться на async API (zbus::azync) и использовать async подписку/Stream для сигналов (может потребовать рефакторинг main → async). Async часто имеет более богатый API для подписок, но меняет модель запуска/интеграции.

- Десериализация `a{sv}`: zvariant::Value может представлять сложные вложенные структуры (Dict, Variant и т.д.). Нужно аккуратно обрабатывать разные представления и логировать сырой debug, если распаковка не сработала.

- Безопасность: не полагаться на небезопасные преобразования путей, корректно декодировать URL (`urlencoding::decode`) и проверять, что путь находится в ожидаемой временной директории (опционально).

----------------------

## Текущая реализация (кратко, файлы)

- src/main.rs — основной код (blocking zbus, вызов Screenshot, логика извлечения uri, копирование). В коде есть несколько вариантов approach (Proxy call, extract_uri_from_map и экспериментальная логика подписки).
- Cargo.toml — зависимости: zbus, serde, async-std (необязательная, для будущего async), urlencoding, dirs
- README.md — краткая инструкция по сборке/использованию

----------------------

## Технологии и стек

- Язык: Rust (edition 2021)
- D-Bus library: zbus (crate `zbus`, blocking and async available)
- Value/deserialization: zvariant
- Утилиты: urlencoding (декодирование file:// URI), dirs (домашняя директория)
- CI/packaging: планируется — GitHub Actions для сборки и release; packaging (.deb/.flatpak) — опционально
- Система интеграции: systemd --user service и helper shell script (screenshot-helper.sh) вызывают бинар

----------------------

## Как тестировать локально

1. На хосте с доступом к D-Bus и порталом (Wayland/Gnome/Flatpak portal):
   - Установить Rust toolchain (rustup).
   - cargo build --manifest-path ~/clawd/claw-screenshot/Cargo.toml --release
   - Запустить: `~/clawd/claw-screenshot/target/release/claw-screenshot`
   - Ожидать в stderr логов и stdout строку `SAVED:/home/mak/clawd/screenshots/<file.png>` при успехе.

2. Интеграция: helper `screenshot-helper.sh` ожидает вывод `SAVED:` и далее берёт путь. Если CLI не вернул ответ — helper fallback'ится на gdbus. После успешной проверки можно удалить fallback.

----------------------

## Следующие шаги (приоритеты)

1. Завершить корректную подписку/чтение сигнала Response в blocking zbus (используя Proxy::builder().path(reply) если доступно, или add_match + фильтрацию) и убедиться, что десериализация (a{sv}) надёжна.
2. Собрать релизный бинарь и интегрировать в helper: установить `~/.local/bin/claw-screenshot` и обновить `screenshot-helper.sh` чтобы использовать его и убрать fallback.
3. Добавить дополнительные проверки безопасности (ограничение директорий, права на файл).
4. Добавить unit/integration тесты (мокающие D-Bus) если возможно, либо документировать manual tests.
5. Опубликовать в GitHub, добавить LICENSE, GitHub Actions для сборки.

----------------------

## Полезные ссылки / референсы

- Freedesktop Portals — Screenshot API: https://flatpak.github.io/xdg-desktop-portal/ (см. интерфейс org.freedesktop.portal.Screenshot и org.freedesktop.portal.Request)
- zbus crate docs: https://docs.rs/zbus (пример использования Proxy и signal handling)
- zvariant crate docs: https://docs.rs/zvariant

----------------------

Если нужно, могу дополнить TASKS.md: добавить конкретные примеры match‑rule, вставить текущие логи ошибок компиляции и патчи, или сразу оформить CHANGELOG/RELEASE инструкцию.
