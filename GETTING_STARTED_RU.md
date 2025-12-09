# Знакомство с joerl

**joerl** — это библиотека модели акторов для Rust, вдохновленная Erlang и названная в честь [Джо Армстронга](https://ru.wikipedia.org/wiki/Армстронг,_Джо_(программист)), создателя Erlang. Если вам когда-либо приходилось строить конкурентные системы на Erlang/OTP и вы думали: «Эх, был бы здесь хоть намек на систему типов», — то вот она, ваша прелесть.

## Зачем вообще joerl?

- **Для эрлангистов**: Та же терминология, те же концепции. Переход настолько плавный, что даже не заметите
- **Production-ready**: Встроенная телеметрия, мониторинг здоровья системы, распределенный обмен сообщениями — все, что нужно для боевого применения
- **Тщательно протестирована**: Обширное property-based тестирование гарантирует корректность. Не верьте на слово — проверьте сами
- **Производительность**: Построена на tokio, работает быстро, не жрет память зря

## Простой пример: Актор-счетчик

Начнем с простейшего примера — актора-счетчика. Ничего особенного, но дает представление:

```rust
use joerl::{Actor, ActorContext, ActorSystem, Message};
use async_trait::async_trait;

// Определяем актор
struct Counter {
    count: i32,
}

#[async_trait]
impl Actor for Counter {
    async fn started(&mut self, ctx: &mut ActorContext) {
        println!("Счетчик запущен с pid {}", ctx.pid());
    }

    async fn handle_message(&mut self, msg: Message, ctx: &mut ActorContext) {
        if let Some(cmd) = msg.downcast_ref::<&str>() {
            match *cmd {
                "increment" => {
                    self.count += 1;
                    println!("[{}] Счет: {}", ctx.pid(), self.count);
                },
                "get" => {
                    println!("[{}] Текущий счет: {}", ctx.pid(), self.count);
                },
                "stop" => {
                    ctx.stop(joerl::ExitReason::Normal);
                },
                _ => {}
            }
        }
    }

    async fn stopped(&mut self, reason: &joerl::ExitReason, ctx: &mut ActorContext) {
        println!("[{}] Счетчик остановлен: {}", ctx.pid(), reason);
    }
}

#[tokio::main]
async fn main() {
    let system = ActorSystem::new();
    let counter = system.spawn(Counter { count: 0 });
    
    counter.send(Box::new("increment")).await.unwrap();
    counter.send(Box::new("increment")).await.unwrap();
    counter.send(Box::new("get")).await.unwrap();
    counter.send(Box::new("stop")).await.unwrap();
    
    // Дадим сообщениям время обработаться
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
}
```

**Ключевые концепции:**
- **Актор**: Инкапсулирует состояние (`count`) и поведение (`handle_message`)
- **ActorSystem**: Среда выполнения, управляющая всеми акторами
- **Передача сообщений**: Type-erased сообщения позволяют отправлять что угодно
- **Lifecycle hooks**: `started()` и `stopped()` для инициализации и очистки

Элегантно, не правда ли? Никаких мьютексов, никаких arc-ов, никакой синхронизации вручную. Модель акторов — штука проверенная десятилетиями.

## Телеметрия и наблюдаемость

Одна из сильных сторон joerl — встроенный мониторинг для production. Не нужно гадать, что пошло не так в три часа ночи. Включаем фичу `telemetry`:

```toml
[dependencies]
joerl = { version = "0.5", features = ["telemetry"] }
metrics-exporter-prometheus = "0.15"
```

Добавляем телеметрию в приложение:

```rust
use joerl::telemetry;
use metrics_exporter_prometheus::PrometheusBuilder;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Запускаем Prometheus exporter
    PrometheusBuilder::new()
        .with_http_listener(([127, 0, 0, 1], 9090))
        .install()?;
    
    telemetry::init();
    
    let system = ActorSystem::new();
    // ... ваши акторы
    
    Ok(())
}
```

Теперь открываем `http://localhost:9090/metrics` и видим:
- **Жизненный цикл акторов**: создание, остановка, паники
- **Пропускная способность**: сообщений отправлено/обработано в секунду
- **Глубина mailbox**: индикаторы backpressure
- **Перезапуски супервизоров**: статистика восстановления после сбоев

**Почему это важно:**
- Мониторинг из коробки, без настройки
- Production debugging без гадания на кофейной гуще
- Готовая интеграция с Grafana/Prometheus
- Поддержка OpenTelemetry для распределенного трейсинга

Не нужно писать костыли, чтобы понять, что творится в системе. Все уже есть.

## Прозрачное распределение

joerl обеспечивает location-transparent messaging: тот же API для локальных и удаленных акторов. Никакой разницы, работаете вы на одной машине или в кластере из десятков нод.

**Запускаем два узла:**

```bash
# Терминал 1: Запускаем EPMD сервер
cargo run --example epmd_server

# Терминал 2: Запускаем узел A
cargo run --example distributed_cluster -- node_a 5001

# Терминал 3: Запускаем узел B
cargo run --example distributed_cluster -- node_b 5002
```

Узлы автоматически обнаруживают друг друга через EPMD (Erlang Port Mapper Daemon). Как в Erlang, только лучше — потому что на Rust.

**Пример кода:**

```rust
use joerl::ActorSystem;

#[tokio::main]
async fn main() {
    // Создаем распределенную систему
    let system = ActorSystem::new_distributed(
        "mynode@localhost",
        5000,
        "127.0.0.1:4369"  // адрес EPMD
    ).await.unwrap();
    
    // Создаем актор — работает точно так же, как локально
    let actor = system.spawn(MyActor::new());
    
    // Отправляем сообщение — работает для локальных И удаленных акторов
    actor.send(Box::new("hello")).await.unwrap();
    
    // Подключаемся к другому узлу
    system.connect_to_node("othernode@localhost").await.unwrap();
    
    // Получаем pid удаленного актора и прозрачно отправляем сообщения
    // ... тот же API, никаких изменений в коде!
}
```

**Ключевые возможности:**
- **Единый API**: `spawn()`, `send()`, `link()` работают идентично
- **Автоматическое обнаружение**: EPMD обрабатывает регистрацию узлов
- **Двунаправленные связи**: Полная семантика соединений в стиле Erlang
- **Сериализация**: Trait-based сериализация сообщений с глобальным реестром

Это **в точности** то, как работает Erlang: написали один раз, разворачивайте где угодно. Location transparency — не маркетинговый термин, а реальность.

## Для разработчиков на Erlang/OTP

Если вы знаете Erlang, вы уже знаете joerl. Вот соответствие:

| Erlang | joerl | Описание |
|--------|-------|----------|
| `spawn/1` | `system.spawn(actor)` | Создать новый процесс |
| `Pid ! Msg` | `actor.send(msg).await` | Отправить сообщение |
| `gen_server:call/2` | `server.call(request).await` | Синхронный RPC |
| `gen_server:cast/2` | `server.cast(msg).await` | Асинхронное сообщение |
| `link/1` | `system.link(pid1, pid2)` | Двунаправленная связь |
| `monitor/2` | `actor.monitor(from)` | Однонаправленный монитор |
| `process_flag(trap_exit, true)` | `ctx.trap_exit(true)` | Обработка сбоев |
| `{'EXIT', Pid, Reason}` | `Signal::Exit { from, reason }` | Сигнал выхода |
| `supervisor:start_link/2` | `spawn_supervisor(&system, spec)` | Дерево супервизоров |

**Пример: Конвертация Erlang gen_server в joerl:**

Erlang:
```erlang
-module(counter).
-behaviour(gen_server).

init([]) -> {ok, 0}.

handle_call(get, _From, State) ->
    {reply, State, State};
handle_call({add, N}, _From, State) ->
    {reply, State + N, State + N}.

handle_cast(increment, State) ->
    {noreply, State + 1}.
```

joerl:
```rust
use joerl::gen_server::{GenServer, GenServerContext};

struct Counter;

#[async_trait]
impl GenServer for Counter {
    type State = i32;
    type Call = CounterCall;
    type Cast = CounterCast;
    type CallReply = i32;

    async fn init(&mut self, _ctx: &mut GenServerContext<'_, Self>) -> Self::State {
        0
    }

    async fn handle_call(
        &mut self,
        call: Self::Call,
        state: &mut Self::State,
        _ctx: &mut GenServerContext<'_, Self>,
    ) -> Self::CallReply {
        match call {
            CounterCall::Get => *state,
            CounterCall::Add(n) => {
                *state += n;
                *state
            }
        }
    }

    async fn handle_cast(
        &mut self,
        cast: Self::Cast,
        state: &mut Self::State,
        _ctx: &mut GenServerContext<'_, Self>,
    ) {
        match cast {
            CounterCast::Increment => *state += 1,
        }
    }
}
```

**Миграция проста:**
1. Маппим callback'и gen_server на методы трейта
2. Используем `async`/`await` там, где Erlang блокируется
3. Сохраняем ментальную модель: акторы, супервизоры, связи, мониторы
4. Получаем type safety и производительность Rust

Единственная сложность — привыкнуть, что компилятор теперь ругается заранее, а не в рантайме в три часа ночи на production.

## Property-Based тестирование: доказательство корректности

joerl использует обширное property-based тестирование с QuickCheck для проверки корректности. Вместо написания отдельных тестовых случаев определяются свойства, которые должны выполняться для **всех** входных данных, затем генерируются сотни случайных тестов.

**Что тестируется:**

1. **Свойства Pid**: Roundtrip сериализации, семантика узлов, равенство
2. **Сериализация сообщений**: Кодирование без потерь, детерминизм, граничные случаи
3. **EPMD протокол**: Все протокольные сообщения, свойства NodeInfo
4. **Жизненный цикл акторов**: Создание, обработка сообщений, завершение
5. **Супервизия**: Стратегии перезапуска, распространение сбоев

**Пример property-теста:**

```rust
use quickcheck_macros::quickcheck;

/// Свойство: Сериализация Pid должна быть без потерь
#[quickcheck]
fn prop_pid_serialization_roundtrip(pid: Pid) -> bool {
    let serialized = serde_json::to_string(&pid).unwrap();
    let deserialized: Pid = serde_json::from_str(&serialized).unwrap();
    pid == deserialized
}
```

QuickCheck генерирует случайные значения `Pid` и проверяет, что свойство выполняется для всех них.

**Запуск property-тестов:**

```bash
# Запустить все property-тесты
cargo test --tests proptest

# Запустить 1000 случайных случаев на свойство
QUICKCHECK_TESTS=1000 cargo test --test proptest_pid

# Запустить конкретный тест
cargo test --test proptest_serialization prop_message_roundtrip
```

**Почему это важно:**
- **Уверенность**: Тесты покрывают случаи, о которых вы бы никогда не подумали вручную
- **Граничные случаи**: Находят угловые ситуации (пустые строки, максимальные значения и т.д.)
- **Предотвращение регрессий**: Неудачные тестовые случаи можно сохранить и воспроизвести
- **Живая документация**: Свойства описывают гарантии кода

Property-based тестирование — это не прихоть. Это доказательство того, что код работает не только на примерах из README, но и в реальной жизни со всеми ее сюрпризами.

Подробности в [PROPERTY_TESTING.md](PROPERTY_TESTING.md).

## Следующие шаги

1. **Учиться на примерах**: Смотрите директорию [`examples/`](joerl/examples/)
   - `counter.rs` — Базовый актор
   - `gen_server_counter.rs` — Паттерн GenServer
   - `supervision_tree.rs` — Отказоустойчивость
   - `telemetry_demo.rs` — Мониторинг
   - `distributed_cluster.rs` — Многоузловые системы

2. **Читайте документацию**: Полная документация API на [docs.rs/joerl](https://docs.rs/joerl)

3. **Разберитесь с супервизией**: Философия "let it crash" из Erlang — ядро joerl

4. **Изучите распределение**: Подробности кластеризации в [DISTRIBUTED.md](DISTRIBUTED.md)

5. **Настройте мониторинг**: Настройка observability в [TELEMETRY.md](TELEMETRY.md)

## Резюме

joerl переносит проверенную модель конкурентного программирования Erlang/OTP в Rust:

- **Просто**: Начните с базовых акторов, добавляйте сложность по мере необходимости
- **Наблюдаемо**: Встроенная телеметрия для debugging в production
- **Распределено**: Написали один раз, разворачивайте на множестве узлов
- **Знакомо**: Прямое соответствие концепциям Erlang
- **Проверено**: Property-based тестирование гарантирует корректность

Приходите ли вы из Erlang или новичок в модели акторов — joerl предоставляет надежный фундамент для построения отказоустойчивых распределенных систем на Rust. Без костылей, без танцев с бубном, без ночных дебагов в три часа утра, когда все упало, и никто не знает почему.

Работает. Проверено. Документировано. Что еще нужно?
