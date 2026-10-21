# reactive-rs

Notificadores reativos thread safe, inspirados no `ChangeNotifier` e no
`ValueNotifier` do Flutter. Sem dependências.

- `Notifier`: avisa os listeners quando você chama `notify_listeners()`.
- `ValueNotifier<T>`: guarda um valor e avisa os listeners quando ele muda.
- `MergeListenable`: junta vários `Listenable` em um só.

## Instalação

O crate ainda não está no crates.io. Use como dependência local:

```toml
[dependencies]
reactive-rs = { path = "../reactive-rs" }
```

## Conceitos

Listeners são closures compartilhadas entre threads:

| Tipo | Definição |
|---|---|
| `Listener` | `Arc<dyn Fn() + Send + Sync>` |
| `ValueListener<T>` | `Arc<dyn Fn(T) + Send + Sync>` |

Os traits `Listenable` e `ValueListenable<T>` oferecem os mesmos três métodos:

- `add_listener(listener)` registra o listener e devolve um id.
- `remove_listener(id)` remove o listener com esse id.
- `subscribe(listener)` registra o listener e devolve um guard que o remove
  quando sai de escopo.

Os traits precisam estar em escopo (`use reactive_rs::Listenable;`) para
chamar esses métodos.

## Notifier

```rust
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use reactive_rs::{Listenable, Notifier};

let notifier = Notifier::new();
let calls = Arc::new(AtomicUsize::new(0));

let id = notifier.add_listener({
    let calls = calls.clone();
    Arc::new(move || {
        calls.fetch_add(1, Ordering::Relaxed);
    })
});

notifier.notify_listeners();
notifier.remove_listener(id);
notifier.notify_listeners(); // ninguém é chamado

assert_eq!(calls.load(Ordering::Relaxed), 1);
```

## Subscriptions: remoção automática

`subscribe` devolve um guard. O listener fica registrado enquanto o guard
existir:

```rust
use std::sync::Arc;

use reactive_rs::{Listenable, Notifier};

let notifier = Notifier::new();

{
    let _subscription = notifier.subscribe(Arc::new(|| println!("notificado")));
    notifier.notify_listeners(); // imprime "notificado"
} // o guard sai de escopo e remove o listener

notifier.notify_listeners(); // não imprime nada
```

> **Cuidado:** `let _ = notifier.subscribe(...)` descarta o guard na hora e o
> listener é removido imediatamente. Use um nome como `_subscription`.

## ValueNotifier

`set` troca o valor e notifica os listeners com uma cópia dele, mas só se o
valor mudou. `get` devolve uma cópia do valor atual.

```rust
use std::sync::{Arc, Mutex};

use reactive_rs::{ValueListenable, ValueNotifier};

let counter = ValueNotifier::new(0);
let seen = Arc::new(Mutex::new(Vec::new()));

let _subscription = counter.subscribe({
    let seen = seen.clone();
    Arc::new(move |value| seen.lock().unwrap().push(value))
});

counter.set(1);
counter.set(1); // mesmo valor: não notifica
counter.set(2);

assert_eq!(counter.get(), 2);
assert_eq!(*seen.lock().unwrap(), [1, 2]);
```

`get` exige `T: Clone`; `set` e `update` exigem `T: Clone + PartialEq`.

## Alterando só um campo com `update`

`update` recebe uma closure que altera o valor no lugar. É útil quando `T` é
uma struct e você quer mudar só um membro dela:

```rust
use std::sync::Arc;

use reactive_rs::{ValueListenable, ValueNotifier};

#[derive(Clone, Debug, PartialEq)]
struct Settings {
    volume: u8,
    muted: bool,
}

let settings = ValueNotifier::new(Settings { volume: 50, muted: false });
let _subscription = settings.subscribe(Arc::new(|s: Settings| println!("{s:?}")));

settings.update(|s| s.volume = 80); // imprime Settings { volume: 80, muted: false }
settings.update(|s| s.volume = 80); // nada mudou: não notifica

assert_eq!(settings.get(), Settings { volume: 80, muted: false });
```

- A closure trabalha numa cópia do valor. Se ela entrar em pânico, o valor
  original fica intacto e ninguém é notificado.
- O valor fica travado enquanto a closure roda, então atualizações
  simultâneas de threads diferentes não se perdem.
- **Não chame `get`, `set` ou `update` do mesmo notifier dentro da closure:**
  isso causa deadlock.

## MergeListenable

Junta vários `Listenable`: um listener registrado no merge é chamado quando
qualquer um dos filhos notifica. Compartilhe os filhos com `Arc`, que também
implementa `Listenable`:

```rust
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use reactive_rs::{Listenable, MergeListenable, Notifier};

let user = Arc::new(Notifier::new());
let cart = Arc::new(Notifier::new());
let any_change = MergeListenable::new(vec![Box::new(user.clone()), Box::new(cart.clone())]);

let changes = Arc::new(AtomicUsize::new(0));
let _subscription = any_change.subscribe({
    let changes = changes.clone();
    Arc::new(move || {
        changes.fetch_add(1, Ordering::Relaxed);
    })
});

user.notify_listeners();
cart.notify_listeners();

assert_eq!(changes.load(Ordering::Relaxed), 2);
```

Quando o `MergeListenable` é destruído, ele remove dos filhos todos os
listeners que registrou neles. Listeners registrados direto nos filhos não são
afetados.

## Entre threads

`Notifier`, `ValueNotifier<T>` (com `T: Send`) e `MergeListenable` são
`Send + Sync`. Para compartilhá-los entre threads, use `Arc`:

```rust
use std::sync::Arc;
use std::thread;

use reactive_rs::ValueNotifier;

#[derive(Clone, PartialEq)]
struct Stats {
    hits: u32,
    misses: u32,
}

let stats = Arc::new(ValueNotifier::new(Stats { hits: 0, misses: 0 }));

let workers: Vec<_> = (0..4)
    .map(|_| {
        let stats = stats.clone();
        thread::spawn(move || {
            for _ in 0..100 {
                stats.update(|s| s.hits += 1);
            }
        })
    })
    .collect();

for worker in workers {
    worker.join().unwrap();
}

assert_eq!(stats.get().hits, 400);
```

## Comportamento

- Os listeners são chamados na ordem em que foram registrados. Registrar o
  mesmo listener duas vezes faz ele ser chamado duas vezes.
- Nenhum lock fica preso enquanto os listeners rodam. Por isso um listener
  pode adicionar ou remover listeners, ou chamar `get`/`set`, sem deadlock.
- Um listener adicionado durante uma notificação só é chamado na próxima. Um
  listener removido durante a notificação, na mesma thread, não é mais
  chamado nela.
- Com várias threads:
  - um listener removido por outra thread no meio de uma notificação ainda
    pode ser chamado uma vez;
  - com vários `set` ao mesmo tempo, as notificações podem chegar
    intercaladas. Se o listener precisa do valor mais recente, chame `get()`.

## Testes

```bash
cargo test
```

Os exemplos deste README também rodam como doctests.
