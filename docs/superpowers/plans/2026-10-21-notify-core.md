# notify: núcleo (parte 1), plano de implementação

> **Para agentes:** SUB-SKILL OBRIGATÓRIA: use superpowers:subagent-driven-development (recomendado) ou superpowers:executing-plans para executar este plano tarefa por tarefa. Os passos usam checkbox (`- [ ]`) para acompanhamento.

**Objetivo:** implementar o núcleo do `notify-rs`, com árvore de elements, reconciliação com keys, reconstrução sob demanda por flags propagadas, `provide`/`get` e `BuilderWidget`, e acrescentar `ValueListenable::get` ao `reactive-rs`.

**Arquitetura:** cada element é dono do widget e dos filhos e embute um `ElementCore` gerenciado pelo framework. Avisos de notifiers, de qualquer thread, ligam flags atômicas que sobem pela árvore, e `App::update` só desce pelos galhos marcados. `provide`/`get` usam um escopo imutável que desce junto com a visita.

**Stack:** Rust edition 2024, workspace Cargo, `reactive-rs` (path). Nenhuma outra crate.

**Spec:** `docs/superpowers/specs/2026-10-21-notify-core-design.md`. Quem executa lê os dois documentos. As seções citadas como "spec §N" são desse arquivo.

## Restrições globais

- Workspace em `jr-rs/Cargo.toml` com `members = ["reactive-rs", "notify-rs"]` e `resolver = "3"`.
- `notify-rs`: `edition = "2024"`, `license = "MIT"`, única dependência `reactive-rs = { path = "../reactive-rs" }`. Não usa dev-dependencies.
- Todas as operações em `Flags` e `Signal` usam `Ordering::SeqCst`. O mutex do `Signal` ignora envenenamento (`unwrap_or_else(PoisonError::into_inner)`).
- Mensagens de pânico exatas:
  - `key duplicada entre irmãos: {key:?}`;
  - `App envenenado por um pânico anterior`;
  - `BuilderWidget::set chamado em um widget sem notifier`;
  - a mensagem do `widget_as` contém `widget_as`.
- Testes unitários ao lado de cada módulo (`#[cfg(test)] mod tests`). Helpers compartilhados ficam em `notify-rs/src/test_support.rs` (`#[cfg(test)] mod test_support;` no `lib.rs`).
- Comentários e documentação de código em inglês, como no `reactive-rs`. Mensagens de commit em português, terminando com `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- Todos os comandos rodam a partir de `/home/johannes/develop/jr-rs`. Não fazer push.
- Ao final de cada tarefa: `cargo fmt --all` sem diferenças e `cargo clippy --workspace --all-targets -- -D warnings` sem avisos (a partir da tarefa 2).

## Foco da revisão

Entradas que o spec implica mas que nenhum requisito testa diretamente. Cada uma ganhou um teste na tarefa dona do código:

1. **Notificação disparada dentro do próprio `build`:** deve agendar outro `update`, sem travar nem perder o aviso. Tarefa 7, `notification_during_build_schedules_another_update`.
2. **Aviso atrasado vindo de um element já removido** (listener ainda vivo em outra thread): deve ser inofensivo, sem pânico e sem reconstruir nada. Tarefa 7, `late_notification_from_removed_element_is_harmless`.
3. **Árvores profundas (1000 níveis):** montar, atualizar e destruir sem estouro de pilha. Tarefa 5, `deep_tree_builds_updates_and_drops`.
4. **Keys com o mesmo valor e tipos diferentes entre irmãos** (`1u32` e `1i64`): são distintas e não causam pânico de key duplicada. Tarefa 6, `equal_values_of_different_key_types_are_distinct`.
5. **`App` sem raiz:** `update()` não faz nada e `debug_tree()` devolve `""`. Tarefa 5, `update_without_root_does_nothing`.

---

## Estrutura de arquivos

| Arquivo | Responsabilidade | Tarefa |
|---|---|---|
| `Cargo.toml` (raiz) | workspace | 2 |
| `.gitignore` (raiz) | `/target` e `.idea/` | 2 |
| `reactive-rs/src/value_listenable.rs`, `value_notifier.rs`, `README.md` | `ValueListenable::get` | 1 |
| `notify-rs/Cargo.toml` | dependências e licença | 2 |
| `notify-rs/src/lib.rs` | módulos e reexportações | 2, 3, 4, 5, 7, 10 |
| `notify-rs/src/key.rs` | `Key` | 2 |
| `notify-rs/src/flags.rs` | `Flags`, `Signal` (internos) | 3 |
| `notify-rs/src/widget.rs` | `Widget`, `BuilderWidget` + impl genérico | 4, 7 |
| `notify-rs/src/element.rs` | `Element`, `ElementCore` | 4, 7, 8 |
| `notify-rs/src/context.rs` | `BuildContext`, escopo, `provide`/`get` | 4, 8 |
| `notify-rs/src/reconcile.rs` | `can_update`, `mount`, `update_child`, `update_children`, `destroy` | 5, 6, 7 |
| `notify-rs/src/app.rs` | `App`, `visit`, `short_type_name` | 5, 7, 8, 9 |
| `notify-rs/src/builder_element.rs` | `BuilderElement<B>` (interno) | 7 |
| `notify-rs/src/test_support.rs` | `Log`, `Leaf`, `Group`, `Probe`, `rc` | 4 |
| `notify-rs/tests/public_api.rs` | uso da API só por fora da crate | 10 |

---

### Tarefa 1: `ValueListenable::get` no `reactive-rs`

**Arquivos:**
- Modificar: `reactive-rs/src/value_listenable.rs` (trait)
- Modificar: `reactive-rs/src/value_notifier.rs` (impl e teste)
- Modificar: `reactive-rs/README.md` (seção "Conceitos")

**Interfaces:**
- Produz: `fn get(&self) -> T` como primeiro método de `ValueListenable<T>`. O impl passa a ser `impl<T: Clone + Send> ValueListenable<T> for ValueNotifier<T>`.

- [ ] **Passo 1: escrever o teste que falha** (em `value_notifier.rs`, `mod tests`)

```rust
#[test]
fn get_works_through_trait_object() {
    let n = ValueNotifier::new(3);
    let listenable: &dyn ValueListenable<i32> = &n;
    assert_eq!(listenable.get(), 3);

    n.set(4);

    assert_eq!(listenable.get(), 4);
}
```

- [ ] **Passo 2: rodar e ver falhar**

Rodar: `cargo test --manifest-path reactive-rs/Cargo.toml get_works_through_trait_object`
Esperado: erro de compilação `no method named get found for reference &dyn ValueListenable<i32>`.

- [ ] **Passo 3: implementar.** Acrescentar `fn get(&self) -> T;` ao trait. No impl, trocar o bound para `T: Clone + Send` e delegar ao `get` inerente com `ValueNotifier::get(self)`, por extenso para não haver recursão. No README, na lista de métodos da seção "Conceitos", acrescentar que `ValueListenable<T>` também tem `get()`, que devolve uma cópia do valor atual.

- [ ] **Passo 4: rodar tudo**

Rodar: `cargo test --manifest-path reactive-rs/Cargo.toml && cargo clippy --manifest-path reactive-rs/Cargo.toml --all-targets -- -D warnings`
Esperado: 27 testes e 6 doctests passando, sem avisos.

- [ ] **Passo 5: commit**

```bash
git add reactive-rs/src/value_listenable.rs reactive-rs/src/value_notifier.rs reactive-rs/README.md
git commit -m "Adiciona get ao trait ValueListenable"
```

---

### Tarefa 2: workspace, crate `notify-rs` e `Key`

**Arquivos:**
- Criar: `Cargo.toml`, `.gitignore` (raiz)
- Remover: `reactive-rs/Cargo.lock` (`git rm`), `notify-rs/Cargo.lock` (não rastreado, `rm`)
- Substituir: `notify-rs/Cargo.toml`, `notify-rs/src/lib.rs`
- Criar: `notify-rs/src/key.rs`

**Interfaces:**
- Produz: `pub struct Key` (`Clone, PartialEq, Eq, Hash, Debug`), `Key::new<V: Hash + Eq + Debug + 'static>(value: V) -> Key`, e `From` para `&str`, `String`, `i32`, `i64`, `u32`, `u64` e `usize`. `&str` é guardado como `String` (spec §6.4). `lib.rs`: `mod key; pub use key::Key;`.

- [ ] **Passo 1: criar o workspace.** `Cargo.toml` da raiz e `notify-rs/Cargo.toml` exatamente como no spec §4. `.gitignore` da raiz com `/target` e `.idea/`. `notify-rs/src/lib.rs` passa a conter só `mod key; pub use key::Key;`. Remover os dois `Cargo.lock` dos membros.

- [ ] **Passo 2: escrever os testes que falham** (`key.rs`)

```rust
#[test]
fn keys_with_same_type_and_value_are_equal() { assert_eq!(Key::new(1u32), Key::new(1u32)); }

#[test]
fn keys_with_different_value_types_differ() { assert_ne!(Key::new(1u32), Key::new(1i64)); }

#[test]
fn str_key_equals_string_key() { assert_eq!(Key::from("a"), Key::new(String::from("a"))); }

#[test]
fn equal_keys_hash_equally() {
    let set: HashSet<Key> = [Key::from(7i32), Key::new(7i32)].into();
    assert_eq!(set.len(), 1);
}

#[test]
fn debug_shows_the_value() {
    assert_eq!(format!("{:?}", Key::from("a")), "\"a\"");
    assert_eq!(format!("{:?}", Key::from(3usize)), "3");
}
```

- [ ] **Passo 3: rodar e ver falhar**

Rodar: `cargo test -p notify-rs`
Esperado: erro de compilação, porque `Key` não existe.

- [ ] **Passo 4: implementar `Key`** como `Rc<dyn KeyValue>`, com um trait privado que compara por `TypeId` + valor e faz o hash do `TypeId` e do valor através de `&mut dyn Hasher`. O `Debug` repassa o `Debug` do valor.

- [ ] **Passo 5: verificar o workspace inteiro**

Rodar: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all --check`
Esperado: os testes do `reactive-rs` e os 5 de `key` passando, sem avisos.

- [ ] **Passo 6: commit**

```bash
git add Cargo.toml Cargo.lock .gitignore notify-rs/Cargo.toml notify-rs/src reactive-rs/Cargo.lock
git commit -m "Cria o workspace e a crate notify-rs com Key"
```

---

### Tarefa 3: `Flags` e `Signal`

**Arquivos:**
- Criar: `notify-rs/src/flags.rs`
- Modificar: `notify-rs/src/lib.rs` (`mod flags;`)

**Interfaces** (tudo `pub(crate)`):
- `struct Signal`:
  - `Signal::new() -> Signal`;
  - `request(&self)`: se `pending` estava desligado, liga e chama o `wake`, clonado sob o lock e chamado fora dele;
  - `clear(&self)`;
  - `is_pending(&self) -> bool`;
  - `set_wake(&self, wake: Arc<dyn Fn() + Send + Sync>)`: substitui o callback; se `pending`, chama o novo uma vez.
- `struct Flags` (campos do spec §9.1):
  - `Flags::new() -> Arc<Flags>`;
  - `attach(&self, parent: Option<Arc<Flags>>, signal: Arc<Signal>)`: preenche os `OnceLock`;
  - `mark_dirty(&self)`: spec §9.1. Sem `signal` anexado, só liga e propaga;
  - `mark_local(&self)`: só `dirty = true`;
  - `mark_within(&self, stop: &Arc<Flags>)`: liga `dirty` e sobe ligando `descendant_dirty`. Para ao chegar em `stop` (sem tocá-lo, comparando com `Arc::ptr_eq`) ou num ancestral já ligado. Não sinaliza;
  - `take_dirty(&self) -> bool` e `take_descendant_dirty(&self) -> bool`: `swap(false)`;
  - `needs_visit(&self) -> bool`: `dirty || descendant_dirty`;
  - `is_dirty(&self) -> bool` e `is_descendant_dirty(&self) -> bool`: leituras.

- [ ] **Passo 1: escrever os testes que falham** (`flags.rs`). Helper de teste `chain(n) -> (Arc<Signal>, Vec<Arc<Flags>>)`, que anexa cada `Flags` ao anterior (o índice 0 é a raiz) e ao mesmo `Signal`.

```rust
#[test]
fn mark_dirty_sets_dirty_and_ancestor_flags() {
    let (_signal, f) = chain(3);
    f[2].mark_dirty();
    assert!(f[2].is_dirty() && f[1].is_descendant_dirty() && f[0].is_descendant_dirty());
    assert!(!f[0].is_dirty() && !f[2].is_descendant_dirty());
}

#[test]
fn mark_dirty_stops_at_first_marked_ancestor() {
    let (_signal, f) = chain(3);
    f[2].mark_dirty();
    f[0].take_descendant_dirty();
    f[2].mark_dirty();
    assert!(!f[0].is_descendant_dirty()); // parou em f[1], que já estava ligado
}

#[test]
fn signal_wakes_once_per_transition() {
    let count = Arc::new(AtomicUsize::new(0));
    let signal = Signal::new();
    let c = count.clone();
    signal.set_wake(Arc::new(move || { c.fetch_add(1, SeqCst); }));
    signal.request(); signal.request(); signal.request();
    assert_eq!(count.load(SeqCst), 1);
    signal.clear();
    signal.request();
    assert_eq!(count.load(SeqCst), 2);
}

#[test]
fn set_wake_with_pending_request_calls_it_at_once() { /* request() e depois set_wake(contador) → contador == 1 */ }

#[test]
fn mark_local_neither_propagates_nor_signals() { /* chain(2); f[1].mark_local() → f[1] sujo, f[0] sem descendant, signal.is_pending() == false */ }

#[test]
fn mark_within_stops_below_stop_and_does_not_signal() {
    let (signal, f) = chain(4);
    f[3].mark_within(&f[1]);
    assert!(f[3].is_dirty() && f[2].is_descendant_dirty());
    assert!(!f[1].is_descendant_dirty() && !f[0].is_descendant_dirty() && !signal.is_pending());
}
```

- [ ] **Passo 2: rodar e ver falhar.** `cargo test -p notify-rs flags`: erro de compilação.
- [ ] **Passo 3: implementar** `flags.rs` conforme as interfaces acima e o spec §9.1.
- [ ] **Passo 4: rodar.** `cargo test -p notify-rs flags`: 6 testes passando. Rodar também clippy e fmt.
- [ ] **Passo 5: commit.** `git add notify-rs/src && git commit -m "Adiciona flags propagadas e sinal de atualização"`

---

### Tarefa 4: `Widget`, `Element`, `ElementCore`, `BuildContext` e helpers de teste

**Arquivos:**
- Criar: `notify-rs/src/widget.rs` (só o trait `Widget` nesta tarefa), `element.rs`, `context.rs`, `test_support.rs`
- Modificar: `notify-rs/src/lib.rs`

**Interfaces:**
- Consome: `Flags` (tarefa 3), `Key` (tarefa 2), `reactive_rs::{Listener, Notifier}`.
- Produz:
  - **`Widget`**, exatamente como no spec §6.1.
  - **`Element`**, como no spec §6.2, com os métodos prontos `widget`, `children` e `mark_dirty`.
  - **`ElementCore`**:
    - campos `pub(crate) widget: Rc<dyn Widget>`, `pub(crate) children: Vec<Box<dyn Element>>` e `pub(crate) flags: Arc<Flags>`;
    - métodos públicos `new(widget: Rc<dyn Widget>) -> Self`, `widget(&self) -> &Rc<dyn Widget>`, `widget_as<W: Widget>(&self) -> Rc<W>`, `children(&self) -> &[Box<dyn Element>]`, `mark_dirty(&self)` e `dirty_listener(&self) -> Listener`.
  - **`BuildContext<'a>`**: nesta tarefa, uma struct sem métodos públicos, com `pub(crate) fn new() -> BuildContext<'static>` e um `PhantomData<&'a mut ()>`. A tarefa 8 troca o conteúdo.
  - **`lib.rs`** reexporta `Widget`, `Element`, `ElementCore` e `BuildContext`.
  - **`test_support.rs`**:
    - `Log` (`Clone + Default`, `push(&self, impl Into<String>)`, `take(&self) -> Vec<String>`);
    - `rc<W: Widget>(w: W) -> Rc<dyn Widget>`;
    - `Leaf { key: Option<Key> }`, com `Leaf::new()` e `Leaf::keyed(impl Into<Key>)`. O element dele não tem filhos;
    - `Group { children: Vec<Rc<dyn Widget>> }`, com `Group::new(children)`. O `build` devolve `children.clone()`;
    - `Probe`, com `Probe::new(name: &'static str, log: &Log)` e os encadeamentos `with_key(impl Into<Key>)`, `with_children(Vec<Rc<dyn Widget>>)` e `with_notifier(Arc<Notifier>)`. O `Widget::notifier` dele devolve o notifier configurado. O element registra `"create {name}"` em `create_element`, `"build {name}"` em cada `build` (devolvendo os filhos) e `"drop {name}"` no `Drop`.

- [ ] **Passo 1: escrever os testes que falham** (`element.rs`)

```rust
#[test]
fn widget_as_returns_the_concrete_widget() {
    let core = ElementCore::new(rc(Leaf::keyed("x")));
    assert_eq!(core.widget_as::<Leaf>().key, Some(Key::from("x")));
}

#[test]
#[should_panic(expected = "widget_as")]
fn widget_as_panics_on_wrong_type() { ElementCore::new(rc(Leaf::new())).widget_as::<Group>(); }

#[test]
fn type_name_is_the_concrete_type() { assert!(rc(Leaf::new()).type_name().ends_with("Leaf")); }

#[test]
fn dirty_listener_marks_the_core() {
    let core = ElementCore::new(rc(Leaf::new()));
    (core.dirty_listener())();
    assert!(core.flags.take_dirty());
}

#[test]
fn dirty_listener_is_send_and_sync() {
    fn assert_send_sync<T: Send + Sync>(_: &T) {}
    assert_send_sync(&ElementCore::new(rc(Leaf::new())).dirty_listener());
}
```

- [ ] **Passo 2: rodar e ver falhar.** `cargo test -p notify-rs element`: erro de compilação.
- [ ] **Passo 3: implementar** os módulos. A conversão em `widget_as` é `Rc<dyn Widget>` → `Rc<dyn Any>` (upcast) → `Rc::downcast::<W>()`. Em caso de erro, entra em pânico com `widget_as::<{nome de W}>: o widget é {type_name do atual}`.
- [ ] **Passo 4: rodar.** `cargo test -p notify-rs`: tudo passando. Rodar também clippy e fmt. Se o clippy acusar código de `test_support` ainda não usado, usar `#[allow(dead_code)]` só nesse módulo.
- [ ] **Passo 5: commit.** `git commit -m "Adiciona Widget, Element, ElementCore e helpers de teste"`

---

### Tarefa 5: `App`, montagem, atualização de filho e destruição

**Arquivos:**
- Criar: `notify-rs/src/reconcile.rs`, `notify-rs/src/app.rs`
- Modificar: `notify-rs/src/lib.rs` (`pub use app::App;`)

**Interfaces:**
- Consome: as tarefas 3 e 4.
- Produz (`reconcile.rs`, `pub(crate)`):
  - `can_update(old: &Rc<dyn Widget>, new: &Rc<dyn Widget>) -> bool`: spec §7.2, usando o `TypeId` de `&**w as &dyn Any`, não o do `Rc`;
  - `mount(widget: Rc<dyn Widget>, parent: Option<&Arc<Flags>>, signal: &Arc<Signal>) -> Box<dyn Element>`: `widget.create_element()`, depois `flags.attach(...)`, depois `mark_local()`;
  - `update_child(old: Option<Box<dyn Element>>, new: Option<Rc<dyn Widget>>, parent: Option<&Arc<Flags>>, signal: &Arc<Signal>) -> Option<Box<dyn Element>>`: a tabela do spec §7.2. No caso `can_update`, troca `core.widget` e chama `mark_local()`;
  - `update_children(old: Vec<Box<dyn Element>>, new: Vec<Rc<dyn Widget>>, parent: &Arc<Flags>, signal: &Arc<Signal>) -> Vec<Box<dyn Element>>`: **nesta tarefa, versão posicional**. Faz `update_child` par a par, e no fim destrói os antigos que sobraram e monta os novos que sobraram;
  - `destroy(element: Box<dyn Element>)`: destrói os filhos do último para o primeiro, recursivamente, e depois solta o element.
- Produz (`app.rs`):
  - `pub struct App` com `pub(crate) root: Option<Box<dyn Element>>` (testes de outros módulos leem a árvore por ele), `signal: Arc<Signal>` e, em `cfg(test)`, `visited: usize`, zerado no início de cada `update`;
  - `App::new()`, `Default`, `set_root(&mut self, Rc<dyn Widget>)` e `update(&mut self)`;
  - `debug_tree(&self) -> String`: cada linha termina com `\n`;
  - `Drop`, que chama `destroy` na raiz;
  - `#[cfg(test)] pub(crate) fn last_visited(&self) -> usize`;
  - `fn short_type_name(full: &str) -> String`: remove todo segmento `identificador::`;
  - `fn visit(element: &mut Box<dyn Element>, signal: &Arc<Signal>, visited: &mut usize)`: os passos 1, 2, 3 e 5 do spec §9.2, sem escopo. Para chamar `build`, usa `BuildContext::new()`. Para reconciliar, troca `core.children` com `mem::take`.
  - `update` faz `signal.clear()` e visita a raiz se `root.core().flags.needs_visit()`.

- [ ] **Passo 1: escrever os testes que falham** (`app.rs`). `log.take()` limpa o registro entre fases.

```rust
#[test]
fn set_root_builds_the_whole_tree() {
    let mut app = App::new();
    app.set_root(rc(Group::new(vec![rc(Leaf::new()), rc(Group::new(vec![rc(Leaf::new())]))])));
    assert_eq!(app.debug_tree(), "Group\n  Leaf\n  Group\n    Leaf\n");
}

#[test]
fn debug_tree_shows_keys() {
    let mut app = App::new();
    app.set_root(rc(Group::new(vec![rc(Leaf::keyed("a")), rc(Leaf::keyed(1i32))])));
    assert_eq!(app.debug_tree(), "Group\n  Leaf #\"a\"\n  Leaf #1\n");
}

#[test]
fn same_type_and_key_reuses_the_element() {
    let (log, mut app) = (Log::default(), App::new());
    app.set_root(rc(Probe::new("p", &log)));
    app.set_root(rc(Probe::new("p", &log)));
    assert_eq!(log.take(), ["create p", "build p", "build p"]);
}

#[test]
fn different_type_replaces_the_element() { /* Probe p → Leaf: ["create p", "build p", "drop p"] */ }

#[test]
fn different_key_replaces_the_element() {
    // Probe p com key 1 → Probe q com key 2
    // ["create p", "build p", "drop p", "create q", "build q"]
}

#[test]
fn same_instance_is_not_rebuilt() {
    let (log, mut app) = (Log::default(), App::new());
    let w = rc(Probe::new("p", &log));
    app.set_root(w.clone());
    app.set_root(w);
    assert_eq!(log.take(), ["create p", "build p"]);
}

#[test]
fn removing_an_element_drops_children_last_to_first_then_parent() {
    // Group[Probe a com filhos [Probe b, Probe c]] → Group[] : depois do primeiro take, ["drop c", "drop b", "drop a"]
}

#[test]
fn dropping_the_app_drops_children_first() { /* mesma árvore; drop(app) → ["drop c", "drop b", "drop a"] */ }

#[test]
fn children_are_matched_by_position() {
    // Group[Probe a, Probe b] → Group[Probe a, Probe b] → ["build a", "build b"]
    // → Group[Probe a] → ["drop b", "build a"]
}

#[test]
fn update_without_changes_visits_nothing() { /* set_root(...); app.update(); last_visited() == 0 */ }

#[test]
fn update_without_root_does_nothing() {
    let mut app = App::new();
    app.update();
    assert_eq!(app.debug_tree(), "");
}

#[test]
fn deep_tree_builds_updates_and_drops() {
    let mut w = rc(Leaf::new());
    for _ in 0..1000 { w = rc(Group::new(vec![w])); }
    let mut app = App::new();
    app.set_root(w);
    assert_eq!(app.debug_tree().lines().count(), 1001);
    app.update();
    drop(app);
}

#[test]
fn short_type_name_strips_module_paths() { assert_eq!(short_type_name("a::b::C<d::E, F>"), "C<E, F>"); }
```

- [ ] **Passo 2: rodar e ver falhar.** `cargo test -p notify-rs app`: erro de compilação.
- [ ] **Passo 3: implementar** `reconcile.rs` e `app.rs` conforme as interfaces acima. `set_root` faz `self.root = update_child(self.root.take(), Some(widget), None, &self.signal)` e depois `self.update()`.
- [ ] **Passo 4: rodar.** `cargo test -p notify-rs`: tudo passando. Rodar também clippy e fmt.
- [ ] **Passo 5: commit.** `git commit -m "Adiciona App com montagem, atualização e destruição de elements"`

---

### Tarefa 6: reconciliação de listas com keys

**Arquivos:**
- Modificar: `notify-rs/src/reconcile.rs` (`update_children` e testes)

**Interfaces:**
- Consome: `update_child`, `mount`, `destroy` e `can_update` (tarefa 5).
- Produz: `update_children` com a mesma assinatura, agora com o algoritmo do spec §7.3 (validação → topo → base → miolo antigo → miolo novo → sobras → base). A validação entra em pânico com `key duplicada entre irmãos: {key:?}`.

- [ ] **Passo 1: escrever os testes que falham** (`reconcile.rs`). Helpers locais:
  - `keyed(log, name, key: i32) -> Rc<dyn Widget>`: um `Probe` com key;
  - `lifecycle(log) -> Vec<String>`: só as entradas que começam com `create` ou `drop`.

```rust
#[test]
fn reordering_keeps_each_element_with_its_key() {
    // [a#1, b#2, c#3] → [c#3, a#1, b#2]
    // lifecycle vazio; log == ["build c", "build a", "build b"]
    // debug_tree == "Group\n  Probe #3\n  Probe #1\n  Probe #2\n"
}

#[test]
fn inserting_in_the_middle_creates_only_the_new_element() { /* [a#1, c#3] → [a#1, b#2, c#3]: lifecycle == ["create b"] */ }

#[test]
fn removing_from_the_middle_drops_only_that_element() { /* [a#1, b#2, c#3] → [a#1, c#3]: lifecycle == ["drop b"] */ }

#[test]
fn inserting_at_both_ends_creates_only_new_elements() { /* [b#2] → [a#1, b#2, c#3]: lifecycle == ["create a", "create c"] */ }

#[test]
fn unkeyed_children_in_the_middle_are_recreated() {
    // [a#1, x, b#2] → [b#2, y, a#1], com x e y Probes sem key
    // lifecycle == ["drop x", "create y"]
}

#[test]
fn same_key_with_different_type_is_recreated() { /* Group[Probe p#1] → Group[Leaf#1]: lifecycle == ["drop p"] */ }

#[test]
#[should_panic(expected = "key duplicada entre irmãos")]
fn duplicate_keys_panic() {
    App::new().set_root(rc(Group::new(vec![rc(Leaf::keyed(1i32)), rc(Leaf::keyed(1i32))])));
}

#[test]
fn equal_values_of_different_key_types_are_distinct() {
    let mut app = App::new();
    app.set_root(rc(Group::new(vec![rc(Leaf::keyed(1u32)), rc(Leaf::keyed(1i64))])));
    assert_eq!(app.debug_tree(), "Group\n  Leaf #1\n  Leaf #1\n");
}
```

- [ ] **Passo 2: rodar e ver falhar.** `cargo test -p notify-rs reconcile`: falham pelo menos `reordering…`, `unkeyed…` e `duplicate_keys_panic`, por causa do casamento posicional.
- [ ] **Passo 3: implementar** o algoritmo do spec §7.3 em `update_children`. No miolo, o mapa é um `HashMap<Key, Box<dyn Element>>`.
- [ ] **Passo 4: rodar.** `cargo test -p notify-rs`: tudo passando, inclusive os testes posicionais da tarefa 5. Rodar também clippy e fmt.
- [ ] **Passo 5: commit.** `git commit -m "Reconcilia listas de filhos com keys"`

---

### Tarefa 7: assinatura de notifiers, `BuilderWidget` e sinal do `App`

**Arquivos:**
- Modificar: `notify-rs/src/element.rs` (campo `subscription` e `Drop`), `reconcile.rs` (assinar em `mount`, trocar em `update_child`), `app.rs` (`on_needs_update`, `needs_update` e testes), `widget.rs` (`BuilderWidget` e impl genérico), `lib.rs`
- Criar: `notify-rs/src/builder_element.rs`

**Interfaces:**
- Consome: as tarefas 3 a 6.
- Produz:
  - **`ElementCore.subscription: Option<(Arc<Notifier>, usize)>`** (`pub(crate)`). O `Drop` de `ElementCore` chama `remove_listener`.
  - **`mount`**, depois do `attach`: se `widget.notifier()` for `Some(n)`, faz `n.add_listener(core.dirty_listener())`.
  - **`update_child`** no caso `can_update`: troca a assinatura quando os notifiers diferem (`Arc::ptr_eq`, ou um dos dois é `None`).
  - **`App::on_needs_update(&mut self, wake: impl Fn() + Send + Sync + 'static)`**, que chama `signal.set_wake(Arc::new(wake))`, e **`App::needs_update(&self) -> bool`**, que devolve `signal.is_pending()`.
  - **`BuilderWidget`** e `impl<B: BuilderWidget> Widget for B`, exatamente como no spec §6.3. O `set` entra em pânico com `BuilderWidget::set chamado em um widget sem notifier`.
  - **`BuilderElement<B>`** (`pub(crate)`): `{ core: ElementCore, _marker: PhantomData<fn() -> B> }`, com `new(widget: Rc<B>)`. O `build` devolve `vec![self.core.widget_as::<B>().build(ctx)]`.
  - **`lib.rs`** reexporta `BuilderWidget`.

- [ ] **Passo 1: escrever os testes que falham.**

Em `builder_element.rs`, com o widget de teste `Counter { notifier: Arc<Notifier>, count: Arc<AtomicI32>, log: Log }` (`Clone`). O `build` dele registra `"build {count}"` e devolve `rc(Leaf::new())`. `Static` é um `BuilderWidget` sem notifier que devolve um `Leaf`.

```rust
#[test]
fn builder_widget_builds_its_child() { /* set_root(Counter) → debug_tree "Counter\n  Leaf\n", log ["build 0"] */ }

#[test]
fn set_changes_then_rebuilds_on_next_update() {
    // set_root(rc(counter.clone())); log.take();
    counter.set(|| { counter.count.fetch_add(1, SeqCst); });
    assert!(app.needs_update());
    app.update();
    assert_eq!(log.take(), ["build 1"]);
}

#[test]
#[should_panic(expected = "BuilderWidget::set chamado em um widget sem notifier")]
fn set_without_notifier_panics() { Static.set(|| {}); }
```

Em `app.rs`:

```rust
#[test]
fn notification_rebuilds_only_the_notified_element() {
    // Group[Probe a com notifier n, Probe b]; log.take(); n.notify_listeners();
    assert!(app.needs_update());
    app.update();
    assert_eq!(log.take(), ["build a"]);
    assert_eq!(app.last_visited(), 2);
}

#[test]
fn notification_from_another_thread_wakes_once_per_transition() {
    // wake incrementa um contador; thread::spawn notifica n duas vezes; join → contador == 1
    // app.update(); n.notify_listeners() → contador == 2
}

#[test]
fn notifications_between_updates_coalesce() { /* 3 avisos; update → log == ["build a"] */ }

#[test]
fn registering_wake_with_pending_update_calls_it_at_once() { /* aviso; depois on_needs_update(contador) → 1 */ }

#[test]
fn changing_the_notifier_moves_the_subscription() {
    // Probe a com n1 → set_root(Probe a com n2)
    // n1 avisa → !needs_update; n2 avisa → needs_update
}

#[test]
fn removed_element_no_longer_wakes_the_app() { /* Probe a com n → set_root(Leaf); n avisa → !needs_update */ }

#[test]
fn deep_notification_visits_only_its_path() {
    // Group[Group[Group[Probe a com n]], Group[Probe b], Probe c]; n avisa; update → last_visited() == 4
}

#[test]
fn notification_during_build_schedules_another_update() {
    // BuilderWidget cujo build chama self.notifier.notify_listeners() só no primeiro build
    // (AtomicBool compartilhado). Depois de set_root: needs_update() == true;
    // update() → o build roda de novo; needs_update() == false
}

#[test]
fn late_notification_from_removed_element_is_harmless() {
    // set_root(Group[Probe a]); log.take();
    let listener = app.root.as_ref().unwrap().children()[0].core().dirty_listener();
    app.set_root(rc(Group::new(vec![])));
    log.take();
    listener();
    app.update();
    assert!(log.take().is_empty());
}

#[test]
fn concurrent_notifications_are_never_lost() {
    // 8 BuilderWidgets "Watcher" { notifier, value: Arc<AtomicU64>, seen: Arc<AtomicU64> },
    // com build fazendo seen.store(value.load()). Dentro de thread::scope, uma thread por
    // watcher faz `for i in 1..=1000 { value.store(i); notifier.notify_listeners() }`, enquanto a
    // thread principal chama app.update() até todas as threads terminarem. Depois:
    // `while app.needs_update() { app.update() }` e assert seen == 1000 para cada watcher.
}
```

- [ ] **Passo 2: rodar e ver falhar.** `cargo test -p notify-rs`: erros de compilação (`BuilderWidget`, `needs_update` e `on_needs_update` não existem).
- [ ] **Passo 3: implementar** conforme as interfaces acima.
- [ ] **Passo 4: rodar.** `cargo test -p notify-rs` e depois `for i in $(seq 20); do cargo test -q -p notify-rs concurrent || break; done`: tudo passando nas 20 repetições. Rodar também clippy e fmt.
- [ ] **Passo 5: commit.** `git commit -m "Assina notifiers dos widgets e adiciona BuilderWidget"`

---

### Tarefa 8: `provide` e `get`

**Arquivos:**
- Modificar: `notify-rs/src/context.rs` (conteúdo real de `BuildContext` e escopo), `element.rs` (campos `provided` e `dependencies`), `app.rs` (`visit` com escopo e visita completa)

**Interfaces:**
- Consome: `Flags::mark_within` (tarefa 3) e `visit` (tarefa 5).
- Produz (`context.rs`, `pub(crate)` salvo indicação):
  - `type Registry = Rc<RefCell<HashMap<usize, Weak<Flags>>>>`, indexado por `Arc::as_ptr(flags) as usize`.
  - `struct ScopeEntry { value: Rc<dyn Any>, dependents: Registry }` (`Clone`) e `type Scope = Rc<HashMap<TypeId, ScopeEntry>>`.
  - `struct NewValue { value: Rc<dyn Any>, eq: fn(&dyn Any, &dyn Any) -> bool }` e `struct Provided { value: Rc<dyn Any>, eq: fn(&dyn Any, &dyn Any) -> bool, dependents: Registry }`.
  - **`BuildContext<'a>`:**
    - `pub(crate) fn new(scope: &'a Scope, flags: &'a Arc<Flags>) -> Self`, que substitui o `new()` da tarefa 4;
    - `pub fn provide<T: PartialEq + 'static>(&mut self, value: T)`;
    - `pub fn get<T: 'static>(&mut self) -> Option<Rc<T>>`;
    - `pub(crate) fn finish(self) -> (HashMap<TypeId, NewValue>, HashMap<TypeId, Option<Rc<dyn Any>>>)`.
  - **`apply_provided(core: &mut ElementCore, new: HashMap<TypeId, NewValue>) -> bool`**: a tabela do spec §8.3. Marca os dependentes com `flags.mark_within(&core.flags)` e devolve `true` se algum tipo foi adicionado.
  - **`child_scope(parent: &Scope, core: &ElementCore) -> Scope`**: devolve o mesmo `Rc` se `core.provided` estiver vazio (spec §8.1).
  - **`dependencies_changed(core: &ElementCore, scope: &Scope) -> bool`**: compara presença e `Rc::ptr_eq`.
  - **`ElementCore`** ganha `pub(crate) provided: HashMap<TypeId, Provided>` e `pub(crate) dependencies: HashMap<TypeId, Option<Rc<dyn Any>>>`.
  - **`visit`** passa a ser `fn visit(element: &mut Box<dyn Element>, scope: &Scope, full: bool, signal: &Arc<Signal>, visited: &mut usize)`. A raiz recebe `Rc::new(HashMap::new())` e `full = false`. Os filhos são visitados com `full || apply_provided(...)`.

- [ ] **Passo 1: escrever os testes que falham** (`context.rs`). Widgets de teste locais:
  - `Theme(i32)`, com `Debug` e `PartialEq`;
  - `Provider { theme: Option<i32>, child: Rc<dyn Widget> }`: o element faz `ctx.provide(Theme(v))` só se for `Some(v)`, e devolve `vec![child]`;
  - `Reader { name: &'static str, log: Log, reads: Arc<AtomicBool>, notifier: Option<Arc<Notifier>> }`: o element registra, a cada `build`, `"{name}={v}"` ou `"{name}=none"` se ler (`reads` ligado), e `"{name} sem ler"` se não ler;
  - `SelfReader { log }`: faz `provide(Theme(5))` e depois `get::<Theme>()` no mesmo `build`, e registra o resultado.

  Nos testes de mudança, o `child` é **o mesmo `Rc`** em todas as chamadas de `set_root`, para que a reconciliação não reconstrua os leitores sozinha.

```rust
#[test]
fn get_finds_the_nearest_provider() { /* Provider(1, Provider(2, Reader r)) → log == ["r=2"] */ }

#[test]
fn element_does_not_see_its_own_provide() { /* set_root(SelfReader) → log == ["self=none"] */ }

#[test]
fn equal_value_rebuilds_nobody() { /* Provider(1, child) → set_root(Provider(1, child)) → log vazio */ }

#[test]
fn changed_value_rebuilds_only_dependents() {
    // child = Group[Reader r, Probe p]; Provider(1, child) → Provider(2, child)
    // log == ["r=2"] (sem "build p")
}

#[test]
fn removed_value_falls_back_to_the_outer_provider() {
    // Provider(1, Provider(2, child)) → Provider(1, Provider(None, child)) → log == ["r=1"]
}

#[test]
fn added_value_rebuilds_readers_below() {
    // Provider(1, Provider(None, child)) → Provider(1, Provider(3, child)) → log == ["r=3"]
    // sem provedor externo: Provider(None, child) → "r=none"; depois Provider(3, child) → ["r=3"]
}

#[test]
fn dependent_registry_does_not_grow() {
    // Reader com notifier próprio, reconstruído 100 vezes (aviso + update)
    // O registro de Theme no core do Provider (via app.root) tem len() == 1
}

#[test]
fn reader_that_stops_reading_is_not_rebuilt_after_registry_clears() {
    // r lê; desliga reads e avisa o notifier dele → ["r sem ler"]
    // Provider(2): no máximo uma reconstrução extra
    // Provider(3): log vazio
}
```

- [ ] **Passo 2: rodar e ver falhar.** `cargo test -p notify-rs context`: erro de compilação (`provide`/`get` não existem).
- [ ] **Passo 3: implementar.** O `visit` segue o spec §9.2 completo:
  - `rebuild = take_dirty() || (full && dependencies_changed)`;
  - `take_descendant_dirty()`;
  - se for reconstruir:
    1. limpa `dependencies` e cria `BuildContext::new(scope, &flags)`;
    2. chama `build` e depois `finish()`;
    3. guarda as `dependencies`;
    4. `added = apply_provided(...)`;
    5. reconcilia os filhos;
  - calcula `child_scope`;
  - visita os filhos com `needs_visit()`, ou todos se `full || added`.
- [ ] **Passo 4: rodar.** `cargo test -p notify-rs`: tudo passando. Rodar também clippy e fmt.
- [ ] **Passo 5: commit.** `git commit -m "Adiciona provide e get no BuildContext"`

---

### Tarefa 9: envenenamento depois de pânico

**Arquivos:**
- Modificar: `notify-rs/src/app.rs`

**Interfaces:**
- Produz: o campo `poisoned: bool` no `App`.
  - `set_root` e `update` entram em pânico com `App envenenado por um pânico anterior` se ele estiver ligado.
  - Os dois ligam o campo ao começar e desligam ao terminar normalmente. Dentro de `set_root`, a chamada interna de `update` não pode acusar o próprio `set_root`: use uma função interna sem a checagem.
  - O `Drop` não muda (spec §10).

- [ ] **Passo 1: escrever os testes que falham.** O widget de teste `Exploder { notifier: Arc<Notifier>, armed: Arc<AtomicBool> }` tem um element cujo `build` entra em pânico quando `armed` está ligado. O helper `panic_message(Box<dyn Any + Send>) -> String` aceita payloads `&str` e `String`.

```rust
#[test]
fn panic_in_build_poisons_the_app() {
    // set_root(Group[Exploder]); armed = true; notifier avisa
    assert!(catch_unwind(AssertUnwindSafe(|| app.update())).is_err());
    let err = catch_unwind(AssertUnwindSafe(|| app.update())).unwrap_err();
    assert_eq!(panic_message(err), "App envenenado por um pânico anterior");
}

#[test]
fn set_root_on_a_poisoned_app_panics() { /* mesmo início; set_root(rc(Leaf::new())) → mesma mensagem */ }

#[test]
fn dropping_a_poisoned_app_releases_subscriptions() {
    // Group[Exploder, Probe a com notifier n]; on_needs_update(contador); envenena;
    // let antes = contador; drop(app); n.notify_listeners(); assert_eq!(contador, antes)
}
```

- [ ] **Passo 2: rodar e ver falhar.** `cargo test -p notify-rs poison`: os dois primeiros falham porque a segunda chamada não entra em pânico.
- [ ] **Passo 3: implementar** conforme as interfaces.
- [ ] **Passo 4: rodar.** `cargo test -p notify-rs`: tudo passando. Rodar também clippy e fmt.
- [ ] **Passo 5: commit.** `git commit -m "Envenena o App depois de um pânico em código de usuário"`

---

### Tarefa 10: API pública vista de fora e verificação final

**Arquivos:**
- Modificar: `notify-rs/src/lib.rs`
- Criar: `notify-rs/tests/public_api.rs`

**Interfaces:**
- Produz: `lib.rs` com uma documentação curta de crate (`//!`) e as reexportações `App`, `BuildContext`, `BuilderWidget`, `Element`, `ElementCore`, `Key`, `Widget` e `pub use reactive_rs::{Listener, Notifier};`. A reexportação do `reactive-rs` é para que quem implementa `Widget::notifier` não precise depender dele diretamente.

- [ ] **Passo 1: escrever o teste de integração** (`tests/public_api.rs`), usando só `notify_rs::*`:
  - **`External`:** um `Widget` com element próprio (`ExternalElement { core: ElementCore, builds: Arc<AtomicUsize>, subscription: Option<(Arc<Notifier>, usize)> }`). No primeiro `build`, ele assina um notifier externo com `self.core.dirty_listener()`. O `Drop` dele remove o listener.
  - **`Other`:** um `Widget` mínimo com element sem filhos, definido no próprio arquivo. Os helpers de `test_support` não são visíveis de fora da crate.
  - **`custom_element_rebuilds_when_external_notifier_fires_from_another_thread`:** `set_root(Rc::new(External { .. }))` → `builds == 1`. Notificar de outra thread → `needs_update()` → `update()` → `builds == 2`. Depois `set_root(Rc::new(Other))` e notificar de novo → `!needs_update()`.
  - **`builder_widget_works_from_outside_the_crate`:** um `BuilderWidget` com notifier; `set` → `update` → o `build` roda de novo.
- [ ] **Passo 2: rodar.** `cargo test -p notify-rs --test public_api`. Esperado: falha de compilação se faltar alguma reexportação. Ajustar o `lib.rs` até passar.
- [ ] **Passo 3: verificação final do workspace**

Rodar: `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && cargo doc --workspace --no-deps`
Esperado: tudo passando, sem avisos, inclusive os doctests do `reactive-rs`.

- [ ] **Passo 4: commit.** `git commit -m "Expõe a API pública do notify-rs e testa uso externo"`

---

## Cobertura do spec

| Seção do spec | Tarefa |
|---|---|
| §4 workspace, Cargo.toml, módulos | 2 (e cada tarefa cria seus módulos) |
| §5 `ValueListenable::get` | 1 |
| §6.1 `Widget` | 4, 7 (`notifier`) |
| §6.2 `Element`/`ElementCore` | 4, 7 (assinatura e `Drop`), 8 (campos de `provide`) |
| §6.3 `BuilderWidget` | 7 |
| §6.4 `Key` | 2 |
| §6.5 `BuildContext` | 4 (casca), 8 |
| §6.6 `App` (`set_root`, `update`, `debug_tree`, `Drop`) | 5; `on_needs_update`/`needs_update` na 7 |
| §7.1 entrada na árvore e marcação local | 5, 7 |
| §7.2 atualizar um filho | 5, 7 (troca de notifier) |
| §7.3 listas com key | 6 |
| §8 `provide`/`get` | 8 |
| §9 flags, sinal, passada, garantias | 3, 5, 7, 8 |
| §10 erros e envenenamento | 4 (`widget_as`), 6 (key duplicada), 7 (`set`), 9 |
| §11 testes 1–10 | 5, 6, 7, 8, 9, 10; o 10 (`reactive-rs`) na tarefa 1 |
