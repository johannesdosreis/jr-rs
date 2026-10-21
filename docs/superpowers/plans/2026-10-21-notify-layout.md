# notify: render e layout (parte 2), plano de implementação

> **Para agentes:** SUB-SKILL OBRIGATÓRIA: use superpowers:subagent-driven-development (recomendado) ou superpowers:executing-plans para executar este plano tarefa por tarefa. Os passos usam checkbox (`- [ ]`) para acompanhamento.

**Objetivo:** dar a cada element tamanho e posição em 3D, com o protocolo de restrições do Flutter, layout incremental com fronteiras de relayout e 16 widgets de layout.

**Arquitetura:**
- O layout mora no próprio `Element`, por ganchos com implementação padrão.
- O framework (`layout.rs`) cuida de cache, fronteiras, flags e validação, e cada element só escreve o próprio algoritmo.
- `App::update` faz build e depois layout.
- As flags `needs_layout`/`descendant_needs_layout` seguem o padrão de "marcar e propagar" da parte 1.

**Stack:** Rust edition 2024, crate `notify-rs`, sem dependências novas.

**Spec:** `docs/superpowers/specs/2026-10-21-notify-layout-design.md`. Quem executa lê os dois documentos. As seções citadas como "spec §N" são desse arquivo. A parte 1 já está implementada; o spec dela é `docs/superpowers/specs/2026-10-21-notify-core-design.md`.

## Restrições globais

- Nenhuma dependência nova no `notify-rs`.
- Geometria em `f32`, com a convenção do spec §2: x para a direita, y para baixo, z na direção de quem olha, e posição = canto mínimo relativo ao pai.
- Mensagens de pânico exatas:
  - `layout impossível: {mensagem}`, com as mensagens da tabela do spec §6;
  - `restrições inválidas: {c:?}`;
  - `{Tipo}: tamanho {s:?} fora das restrições {c:?}`, em que `Tipo` é o nome curto do widget.
- **Testes de debug e de release:**
  - comportamento de debug: `#[cfg(debug_assertions)]` com `#[should_panic(expected = ...)]`;
  - comportamento de release: `#[cfg(not(debug_assertions))]`;
  - toda tarefa que tem os dois roda `cargo test` **e** `cargo test --release` no filtro dela.
- Testes ao lado de cada módulo. Helpers compartilhados ficam em `notify-rs/src/test_support.rs`.
- Comentários de código em inglês. Commits em português, terminando com `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- Comandos rodam a partir de `/home/johannes/develop/jr-rs`, numa branch `notify-layout`. Não fazer push.
- Ao final de cada tarefa: `cargo fmt --all --check` sem diferenças e `cargo clippy --workspace --all-targets -- -D warnings` sem avisos.

**Decisões deste plano** (o spec não cobre):
- **`ElementCore::needs_layout_listener() -> Listener`** (tarefa 3). O spec §3.2 diz que `mark_needs_layout` pode ser chamado de qualquer thread, mas `ElementCore` não é `Send`. Este listener é o caminho para outras threads, igual ao `dirty_listener` da parte 1.
- **`parent_data()` dos widgets prontos** devolve uma referência a um campo do próprio widget, convertendo `self.core().widget()` para `&dyn Any` com `downcast_ref`. Não copia nada para o element.

## Foco da revisão

Cada item tem um teste na tarefa dona do código:

1. **Janela de tamanho zero** (`set_size(Size::ZERO)`) com `Padding` maior que o espaço: nada entra em pânico, e tudo fica com tamanho 0. Tarefa 5, `padding_in_a_zero_size_window`.
2. **`Expanded`/`Positioned` fora de `Flex`/`Stack`:** passam o filho direto, sem pânico. Tarefa 6, `expanded_outside_flex_passes_through`, e tarefa 7, `positioned_outside_stack_passes_through`.
3. **`Center`/`Align` num eixo sem limite** (dentro de uma `Row`): o tamanho nesse eixo é o do filho. Tarefa 6, `center_inside_row_takes_child_width`.
4. **Filho novo inserido no meio de uma lista** dentro de um pai de restrições exatas: dimensionado e posicionado no mesmo `update`. Tarefa 6, `inserted_child_is_laid_out_in_the_same_update`.
5. **Reordenar filhos com key numa `Row`:** os elements são reaproveitados, e as posições acompanham a nova ordem. Tarefa 6, `reordered_keyed_children_get_new_offsets`.

---

## Estrutura de arquivos

| Arquivo | Responsabilidade | Tarefa |
|---|---|---|
| `notify-rs/src/geometry.rs` | `Axis`, `Size`, `Offset`, `Constraints`, `EdgeInsets`, `Alignment` | 1 |
| `notify-rs/src/flags.rs` | flags e marcação de layout | 2 |
| `notify-rs/src/layout.rs` | `layout_element`, `relayout_descendants`, `default_layout`, validações, `impossible_layout` | 3 |
| `notify-rs/src/element.rs` | ganchos no `Element`, campos e métodos no `ElementCore` | 3 |
| `notify-rs/src/app.rs` | `set_size`, passada de layout, `debug_layout`, marcação na passada de build | 3, 4 |
| `notify-rs/src/test_support.rs` | `LayoutProbe`, `layout_of`, `sized` | 3 |
| `notify-rs/src/widgets/mod.rs` | declara e reexporta os widgets | 5 |
| `notify-rs/src/widgets/constrained.rs`, `padding.rs`, `align.rs`, `aspect_ratio.rs` | widgets de um filho | 5 |
| `notify-rs/src/widgets/flex.rs` | `Flex`, `Row`, `Column`, `Expanded`, `Flexible`, `Spacer` | 6 |
| `notify-rs/src/widgets/stack.rs` | `Stack`, `Positioned` | 7 |
| `notify-rs/src/widgets/wrap.rs` | `Wrap` | 8 |
| `notify-rs/src/widgets/grid.rs` | `Grid` | 9 |
| `notify-rs/tests/layout_api.rs` | element de layout próprio, usando só a API pública | 10 |
| `notify-rs/src/lib.rs` | módulos e reexportações | 1, 3, 5–10 |

---

### Task 1: geometria

**Arquivos:**
- Criar: `notify-rs/src/geometry.rs`
- Modificar: `notify-rs/src/lib.rs` (`mod geometry; pub use geometry::{Alignment, Axis, Constraints, EdgeInsets, Offset, Size};`)

**Interfaces** (produz exatamente o spec §2; todos `Copy + PartialEq + Debug`, e `Size`, `Offset` e `EdgeInsets` também `Default`):
- **`Axis { X, Y, Z }`** (também `Eq + Hash`), com `pub const ALL: [Axis; 3]`.
- **`Size`**:
  - `new(w, h, d)`, `flat(w, h)` e `ZERO`, todos `const`;
  - `along(self, Axis) -> f32` e `with_axis(self, Axis, f32) -> Size`.
- **`Offset`**: o mesmo conjunto, com os campos `x`, `y`, `z`.
- **`Constraints`** (campos públicos do spec):
  - construtores `tight`, `loose`, `expand` e `unbounded`;
  - por eixo: `min(&self, Axis)`, `max(&self, Axis)`, `has_bounded(&self, Axis)`, `with_axis(self, Axis, min, max)` e `tighten(self, Axis, v)`;
  - globais: `is_tight`, `constrain(&self, Size) -> Size`, `loosen`, `enforce(self, other)`, `deflate(self, EdgeInsets)`, `biggest` e `smallest`.
- **`EdgeInsets`**:
  - `ZERO`, `all(v)` e `symmetric(horizontal, vertical)`, que afetam só o plano;
  - encadeados `left/top/right/bottom(self, v)` e `with_depth(self, front, back)`;
  - totais `horizontal()`, `vertical()` e `depth()`.
- **`Alignment`**:
  - as 9 constantes do plano (com z = 0), mais `BACK` e `FRONT`;
  - `with_z`, `along` e `offset_of(&self, child: Size, parent: Size) -> Offset`.

- [ ] **Passo 1: escrever os testes que falham** (`geometry.rs`). `S(w, h, d)` é `Size::new`.

```rust
#[test]
fn tight_and_loose() {
    let t = Constraints::tight(Size::new(10.0, 20.0, 3.0));
    assert!(t.is_tight());
    assert_eq!((t.min(Axis::Y), t.max(Axis::Z)), (20.0, 3.0));
    let l = Constraints::loose(Size::new(10.0, 20.0, 3.0));
    assert!(!l.is_tight() && l.min(Axis::X) == 0.0 && l.max(Axis::X) == 10.0);
    assert!(!Constraints::unbounded().has_bounded(Axis::X));
}

#[test]
fn constrain_clamps_each_axis() {
    let c = Constraints::loose(Size::new(100.0, 50.0, 0.0));
    assert_eq!(c.constrain(Size::new(150.0, 20.0, 5.0)), Size::new(100.0, 20.0, 0.0));
}

#[test]
fn enforce_keeps_result_inside_other() {
    let r = Constraints::tight(Size::new(50.0, 50.0, 0.0)).enforce(Constraints::loose(Size::new(40.0, 100.0, 0.0)));
    assert_eq!((r.min_width, r.max_width, r.min_height, r.max_height), (40.0, 40.0, 50.0, 50.0));
    let c = Constraints::loose(Size::new(80.0, 60.0, 0.0));
    assert_eq!(Constraints::expand().enforce(c), Constraints::tight(c.biggest()));
}

#[test]
fn deflate_subtracts_insets_without_going_negative() {
    let d = Constraints::tight(Size::new(100.0, 100.0, 10.0)).deflate(EdgeInsets::all(10.0).with_depth(2.0, 3.0));
    assert_eq!(d, Constraints::tight(Size::new(80.0, 80.0, 5.0)));
    let z = Constraints::loose(Size::new(5.0, 5.0, 0.0)).deflate(EdgeInsets::all(10.0));
    assert_eq!((z.max_width, z.min_width), (0.0, 0.0));
    assert_eq!(Constraints::unbounded().deflate(EdgeInsets::all(10.0)).max_width, f32::INFINITY);
}

#[test]
fn loosen_tighten_biggest_smallest() {
    let c = Constraints::tight(Size::new(10.0, 20.0, 0.0)).loosen();
    assert_eq!(c.smallest(), Size::ZERO);
    assert_eq!(c.biggest(), Size::new(10.0, 20.0, 0.0));
    assert_eq!(Constraints::loose(Size::new(100.0, 0.0, 0.0)).tighten(Axis::X, 150.0).min_width, 100.0);
    assert_eq!(Constraints::unbounded().with_axis(Axis::Y, 5.0, 9.0).max(Axis::Y), 9.0);
}

#[test]
fn size_and_offset_along_axes() {
    let s = Size::new(1.0, 2.0, 3.0);
    assert_eq!((s.along(Axis::X), s.along(Axis::Z)), (1.0, 3.0));
    assert_eq!(s.with_axis(Axis::Y, 9.0), Size::new(1.0, 9.0, 3.0));
    assert_eq!(Size::flat(4.0, 5.0), Size::new(4.0, 5.0, 0.0));
    assert_eq!(Offset::ZERO.with_axis(Axis::Z, 2.0).along(Axis::Z), 2.0);
}

#[test]
fn edge_insets_all_is_planar() {
    let e = EdgeInsets::all(4.0);
    assert_eq!((e.horizontal(), e.vertical(), e.depth()), (8.0, 8.0, 0.0));
    assert_eq!(EdgeInsets::symmetric(1.0, 2.0).with_depth(3.0, 4.0).depth(), 7.0);
    assert_eq!(EdgeInsets::ZERO.left(5.0).horizontal(), 5.0);
}

#[test]
fn alignment_offsets_in_three_axes() {
    let (child, parent) = (Size::new(20.0, 10.0, 2.0), Size::new(100.0, 50.0, 10.0));
    assert_eq!(Alignment::CENTER.offset_of(child, parent), Offset::new(40.0, 20.0, 4.0));
    assert_eq!(Alignment::BOTTOM_RIGHT.offset_of(child, parent), Offset::new(80.0, 40.0, 4.0));
    assert_eq!(Alignment::FRONT.offset_of(child, parent).z, 8.0);
    assert_eq!(Alignment::TOP_LEFT.with_z(-1.0).offset_of(child, parent), Offset::ZERO);
}
```

- [ ] **Passo 2: rodar e ver falhar.** `cargo test -p notify-rs --lib geometry`: erro de compilação.
- [ ] **Passo 3: implementar** `geometry.rs`, seguindo as fórmulas do spec §2. `enforce` limita cada mínimo e cada máximo de `self` ao intervalo `[other.min, other.max]` do mesmo eixo.
- [ ] **Passo 4: rodar.** `cargo test -p notify-rs --lib geometry`: 8 testes passando. Rodar também clippy e fmt.
- [ ] **Passo 5: commit.** `git commit -m "Adiciona a geometria 3D de layout"`

---

### Task 2: flags de layout

**Arquivos:**
- Modificar: `notify-rs/src/flags.rs`

**Interfaces** (`pub(crate)`):
- **Campos novos de `Flags`:** `needs_layout` (`new()` começa com `true`), `descendant_needs_layout` e `relayout_boundary`, todos `AtomicBool` com `SeqCst`.
- **Marcação** (spec §4.3):
  - `mark_needs_layout(&self)`, com `signal.request()` no fim;
  - `mark_needs_layout_quiet(&self)`, sem sinal.
- **Estado:**
  - `set_relayout_boundary(&self, bool)`;
  - `needs_layout`, `descendant_needs_layout` e `needs_layout_visit` (um dos dois), todos `(&self) -> bool`;
  - `take_needs_layout` e `take_descendant_needs_layout`, ambos `(&self) -> bool`, com `swap(false)`.
- **`parent(&self) -> Option<&Arc<Flags>>`.**

**Algoritmo da marcação:**
1. Partindo de `self`: se `needs_layout.swap(true)` devolver `true`, para.
2. Se o nó é fronteira ou não tem pai, liga `descendant_needs_layout` nos ancestrais dele, parando no primeiro já ligado, e para.
3. Caso contrário, repete a partir do pai.

- [ ] **Passo 1: escrever os testes que falham** (`flags.rs`, reaproveitando o helper `chain`)

```rust
#[test]
fn new_flags_need_layout() {
    let (_signal, f) = chain(1);
    assert!(f[0].needs_layout());
}

#[test]
fn needs_layout_propagates_up_to_the_boundary() {
    let (_signal, f) = chain(4);
    for flags in &f { flags.take_needs_layout(); }
    f[1].set_relayout_boundary(true);
    f[3].mark_needs_layout_quiet();
    assert!(f[3].needs_layout() && f[2].needs_layout() && f[1].needs_layout());
    assert!(!f[0].needs_layout() && f[0].descendant_needs_layout());
    assert!(!f[1].descendant_needs_layout());
}

#[test]
fn root_acts_as_boundary() {
    let (_signal, f) = chain(2);
    for flags in &f { flags.take_needs_layout(); }
    f[1].mark_needs_layout_quiet();
    assert!(f[0].needs_layout());
}

#[test]
fn marking_stops_at_an_already_marked_element() {
    let (_signal, f) = chain(3);
    for flags in &f { flags.take_needs_layout(); }
    f[1].mark_needs_layout_quiet();
    f[0].take_needs_layout();
    f[2].mark_needs_layout_quiet();
    assert!(!f[0].needs_layout());
}

#[test]
fn quiet_marking_does_not_signal() {
    let (signal, f) = chain(2);
    for flags in &f { flags.take_needs_layout(); }
    f[1].mark_needs_layout_quiet();
    assert!(!signal.is_pending());
    f[1].take_needs_layout();
    f[0].take_needs_layout();
    f[1].mark_needs_layout();
    assert!(signal.is_pending());
}

#[test]
fn take_and_visit_for_layout() {
    let (_signal, f) = chain(2);
    assert!(f[1].take_needs_layout() && !f[1].needs_layout_visit());
    f[1].set_relayout_boundary(true);
    f[1].mark_needs_layout_quiet();
    assert!(f[0].needs_layout_visit() && f[0].take_descendant_needs_layout());
}
```

- [ ] **Passo 2: rodar e ver falhar.** `cargo test -p notify-rs --lib flags`: erro de compilação.
- [ ] **Passo 3: implementar.** Se o clippy acusar código ainda não usado, use `#[allow(dead_code)]` temporário nos métodos novos e registre isso no ledger. A tarefa 3 remove.
- [ ] **Passo 4: rodar.** `cargo test -p notify-rs --lib flags`: os 9 testes antigos e os 6 novos passando. Rodar também clippy e fmt.
- [ ] **Passo 5: commit.** `git commit -m "Adiciona flags de layout com propagação até a fronteira"`

---

### Task 3: motor de layout, API no `Element` e no `App`

**Arquivos:**
- Criar: `notify-rs/src/layout.rs`
- Modificar: `notify-rs/src/element.rs`, `app.rs`, `test_support.rs`, `lib.rs` (`mod layout; pub use layout::impossible_layout;`)

**Interfaces:**
- Consome: a geometria (tarefa 1) e as flags (tarefa 2).
- **`Element`** ganha os ganchos do spec §3.1:
  - `layout(&mut self, Constraints) -> Size`, com padrão `crate::layout::default_layout(self.core_mut(), constraints)`;
  - `sized_by_parent(&self) -> bool` (padrão `false`);
  - `parent_data(&self) -> Option<&dyn Any>` (padrão `None`);
  - `size(&self) -> Size` e `offset(&self) -> Offset`, ambos lendo o core.
- **`ElementCore`:**
  - campos novos `pub(crate) constraints: Option<Constraints>`, `parent_uses_size: bool`, `size: Size` e `offset: Offset`;
  - métodos do spec §3.2: `layout_child`, `set_child_offset`, `size`, `offset` e `mark_needs_layout`;
  - **`needs_layout_listener(&self) -> Listener`**, decisão deste plano.
- **`layout.rs`:**
  - `pub fn impossible_layout(message: &str)`: spec §3.4;
  - `pub(crate) fn layout_element(element: &mut Box<dyn Element>, constraints: Constraints, parent_uses_size: bool) -> Size`: spec §4.4. A fronteira é `!parent_uses_size || sized_by_parent() || constraints.is_tight()`, e a raiz entra como fronteira por receber `parent_uses_size = false`;
  - `pub(crate) fn relayout_descendants(element: &mut Box<dyn Element>)`;
  - `pub(crate) fn default_layout(core: &mut ElementCore, constraints: Constraints) -> Size`: spec §3.1;
  - funções internas `normalize_constraints` e `validate_size`, conforme o spec §6. A mensagem de tamanho usa `crate::app::short_type_name`, que passa a ser `pub(crate)`.
- **`App`:**
  - campo `size: Option<Size>`;
  - `pub fn set_size(&mut self, size: Size)`: guarda o tamanho e, se ele mudou, chama `signal.request()`;
  - `rebuild_dirty` passa a fazer, depois da visita de build, `layout_element(root, Constraints::tight(size), false)` quando há raiz e tamanho;
  - `pub fn debug_layout(&self) -> String`, no formato do spec §3.3.
- **`test_support.rs`:**
  - `LayoutProbe::new(name, &log)`, com os encadeamentos:
    - `.size(Size)` (padrão `ZERO`);
    - `.children(Vec<Rc<dyn Widget>>)`;
    - `.child_constraints(Constraints)`: se ausente, repassa as restrições recebidas;
    - `.parent_uses_size(bool)` (padrão `true`);
    - `.sized_by_parent(bool)`;
    - `.notifier(Arc<Notifier>)`;
    - `.parent_data(u32)`;
    - `.raw_size()`: devolve o tamanho sem `constrain`.
  - O element do `LayoutProbe`:
    - registra `"layout {name}"` a cada `layout`;
    - faz o layout de cada filho com as restrições configuradas e o `parent_uses_size` configurado, posicionando todos em `Offset::ZERO`;
    - devolve `c.constrain(size)`, ou só `size` com `raw_size`;
    - o `parent_data` dele é `Option<&u32>` lido do widget;
    - o `build` não registra nada.
  - `layout_of(size: Size, root: Rc<dyn Widget>) -> String`: cria um `App`, chama `set_size`, `set_root` e devolve o `debug_layout()`.
  - `sized(w: f32, h: f32) -> Rc<dyn Widget>`: fica disponível a partir da tarefa 5, porque depende de `SizedBox`.

- [ ] **Passo 1: escrever os testes que falham** (`layout.rs`, `mod tests`). `probe(name)` é `LayoutProbe::new(name, &log)`, e `L(w, h)` é `Constraints::loose(Size::flat(w, h))`.

```rust
#[test]
fn update_lays_out_the_tree_with_window_constraints() {
    let (log, mut app) = (Log::default(), App::new());
    app.set_size(Size::flat(100.0, 50.0));
    app.set_root(rc(probe("root").size(Size::flat(100.0, 50.0)).child_constraints(L(100.0, 50.0))
        .children(vec![rc(probe("a").size(Size::flat(30.0, 20.0)))])));
    assert_eq!(app.debug_layout(), "LayoutProbe 100x50x0 @ 0,0,0\n  LayoutProbe 30x20x0 @ 0,0,0\n");
    assert_eq!(log.take(), ["layout root", "layout a"]);
}

#[test]
fn no_layout_without_size() { /* set_root sem set_size → log sem nenhum "layout" */ }

#[test]
fn update_without_changes_does_not_lay_out() { /* depois do primeiro layout, log.take(); app.update() → log vazio */ }

#[test]
fn same_size_does_nothing_and_new_size_relayouts() {
    // set_size(100x50) + set_root(probe root) → take; set_size(100x50); update → vazio
    // set_size(200x50); assert!(app.needs_update()); update → ["layout root"]
}

#[test]
fn needs_layout_propagates_to_the_nearest_boundary() {
    // root (child_constraints tight 50x50) → mid (child_constraints L(50, 50)) → leaf
    // take; leaf.core().mark_needs_layout() (via app.root…children()[0].children()[0]); update
    // log == ["layout mid", "layout leaf"]
}

#[test]
fn parent_not_using_size_makes_a_boundary() {
    // root(tight 50x50) → mid(L(50,50), parent_uses_size(false)) → leaf; marca leaf → ["layout leaf"]
}

#[test]
fn sized_by_parent_makes_a_boundary() {
    // root(tight) → mid(L(50,50)) → leaf.sized_by_parent(true); marca leaf → ["layout leaf"]
}

#[test]
fn clean_child_with_same_constraints_is_skipped() {
    // root(L(100,50)) → mid(L(100,50)) → leaf; marca mid → ["layout root", "layout mid"]
}

#[test]
fn needs_layout_listener_works_from_another_thread() {
    // listener = leaf.core().needs_layout_listener(); thread::spawn(move || listener()).join()
    // assert!(app.needs_update()); update → log contém "layout leaf"
}

#[test]
fn default_layout_overlaps_children_and_takes_largest() {
    // set_size 100x100; root probe(child_constraints L(100,100)) → Group[probe a 10x30, probe b 20x5]
    // a linha do Group no debug_layout == "  Group 20x30x0 @ 0,0,0"
}

#[cfg(debug_assertions)]
#[test]
#[should_panic(expected = "restrições inválidas")]
fn invalid_constraints_panic_in_debug() {
    // probe root com child_constraints min_width 10 > max_width 5
}

#[cfg(not(debug_assertions))]
#[test]
fn invalid_constraints_are_normalized_in_release() {
    // mesmo caso; o filho (size 8x8) recebe min_width rebaixado para 5 → largura 5
}

#[cfg(debug_assertions)]
#[test]
#[should_panic(expected = "fora das restrições")]
fn size_outside_constraints_panics_in_debug() { /* filho com size(500x500).raw_size() dentro de L(100,100) */ }

#[cfg(not(debug_assertions))]
#[test]
fn size_outside_constraints_is_clamped_in_release() { /* o mesmo filho fica 100x100 */ }

#[cfg(debug_assertions)]
#[test]
#[should_panic(expected = "layout impossível: teste")]
fn impossible_layout_panics_in_debug() { impossible_layout("teste"); }

#[cfg(not(debug_assertions))]
#[test]
fn impossible_layout_returns_in_release() { impossible_layout("teste"); }
```

- [ ] **Passo 2: rodar e ver falhar.** `cargo test -p notify-rs --lib layout`: erro de compilação.
- [ ] **Passo 3: implementar** conforme as interfaces e o spec §§3, 4.4 e 6. Remover o `allow` temporário da tarefa 2.
- [ ] **Passo 4: rodar.**
  - `cargo test -p notify-rs --lib`: os testes antigos e os novos passando;
  - `cargo test -p notify-rs --lib --release layout`: os testes de release passando;
  - clippy e fmt.
- [ ] **Passo 5: commit.** `git commit -m "Adiciona o motor de layout incremental e a API de layout"`

---

### Task 4: marcação de layout na passada de build

**Arquivos:**
- Modificar: `notify-rs/src/app.rs` (função `visit`) e testes em `layout.rs`

**Interfaces:**
- Consome: `Flags::mark_needs_layout_quiet` e `Flags::parent` (tarefa 2) e `Element::parent_data` (tarefa 3).
- Produz o comportamento do spec §4.3. Em `visit`, depois de reconstruir um element:
  1. `flags.mark_needs_layout_quiet()`;
  2. se `element.parent_data().is_some()`, também `flags.parent().map(|p| p.mark_needs_layout_quiet())`.

- [ ] **Passo 1: escrever os testes que falham** (`layout.rs`)

```rust
#[test]
fn rebuilt_element_is_laid_out_again() {
    // janela 100x100; root(tight 50x50) → mid(L(50,50)) → leaf.notifier(n); take; n.notify_listeners(); update
    // log == ["layout mid", "layout leaf"]   (mid é fronteira por receber restrições exatas)
}

#[test]
fn rebuilt_element_with_parent_data_relayouts_its_parent() {
    // janela 100x100; root(tight 50x50) → mid(tight 50x50) → leaf.notifier(n).parent_data(1)
    // mid e leaf recebem restrições exatas, então são fronteiras; mesmo assim, como leaf tem parent_data, mid refaz o layout
    // take; n avisa; update → log == ["layout mid", "layout leaf"]
}
```

- [ ] **Passo 2: rodar e ver falhar.** `cargo test -p notify-rs --lib rebuilt_element`: falha, porque o log fica vazio e nenhum layout é refeito.
- [ ] **Passo 3: implementar** a marcação em `visit`.
- [ ] **Passo 4: rodar.** `cargo test -p notify-rs --lib`: tudo passando. Rodar também clippy e fmt.
- [ ] **Passo 5: commit.** `git commit -m "Marca layout ao reconstruir elements"`

---

### Task 5: widgets de um filho

**Arquivos:**
- Criar: `notify-rs/src/widgets/mod.rs`, `constrained.rs`, `padding.rs`, `align.rs`, `aspect_ratio.rs`
- Modificar: `notify-rs/src/lib.rs` (`mod widgets; pub use widgets::*;`) e `test_support.rs` (`sized`)

**Interfaces** (estilo do spec §5: campos privados, `new`, encadeados, filhos `Rc<dyn Widget>`):
- **`ConstrainedBox::new(Constraints)`**, com `.child(Rc<dyn Widget>)`.
- **`SizedBox`**:
  - `new()`, `expand()` e `shrink()`;
  - `.width(f32)`, `.height(f32)`, `.depth(f32)` e `.child(...)`;
  - o element reaproveita a lógica do `ConstrainedBox`, por uma função `fn constrained_layout(core, extra) -> Size` no próprio arquivo.
- **`Padding::new(EdgeInsets, Rc<dyn Widget>)`.**
- **`Align::new(Alignment, Rc<dyn Widget>)`**, com `.width_factor/.height_factor/.depth_factor(f32)`, e **`Center::new(Rc<dyn Widget>)`**. Os dois compartilham `fn align_layout(core, alignment, factors: [Option<f32>; 3], c) -> Size`.
- **`AspectRatio::new(f32, Rc<dyn Widget>)`.**
- As fórmulas de cada widget estão no spec §§5.1, 5.2, 5.3 e 5.6.
- `sized(w, h) = rc(SizedBox::new().width(w).height(h))`.

- [ ] **Passo 1: escrever os testes que falham.** Cada arquivo tem os seus. `W = Size::flat(100.0, 100.0)`.

```rust
// constrained.rs
#[test]
fn constrained_box_enforces_extra_constraints() {
    let c = Constraints::loose(Size::flat(50.0, 50.0));
    assert_eq!(layout_of(W, rc(Center::new(rc(ConstrainedBox::new(c).child(sized(80.0, 80.0)))))),
        "Center 100x100x0 @ 0,0,0\n  ConstrainedBox 50x50x0 @ 25,25,0\n    SizedBox 50x50x0 @ 0,0,0\n");
}
#[test]
fn constrained_box_without_child_takes_smallest() {
    // Constraints { min_width: 10, min_height: 20, demais 0..∞ } dentro de Center
    // → "  ConstrainedBox 10x20x0 @ 45,40,0"
}
#[test]
fn sized_box_has_exact_size() { /* Center(sized(30,40)) → "  SizedBox 30x40x0 @ 35,30,0" */ }
#[test]
fn sized_box_expand_and_shrink() { /* Center(expand) → 100x100 @ 0,0,0; Center(shrink) → 0x0x0 @ 50,50,0 */ }
#[test]
fn sized_box_constrains_its_child() { /* Center(SizedBox.width(30).child(sized(10,10))) → SizedBox 30x10x0 e filho 30x10x0 */ }

// padding.rs
#[test]
fn padding_deflates_and_offsets() {
    assert_eq!(layout_of(W, rc(Center::new(rc(Padding::new(EdgeInsets::all(10.0), sized(30.0, 20.0)))))),
        "Center 100x100x0 @ 0,0,0\n  Padding 50x40x0 @ 25,30,0\n    SizedBox 30x20x0 @ 10,10,0\n");
}
#[test]
fn padding_with_depth() {
    // volume Size::new(100,100,50); Center(Padding(ZERO.with_depth(5,3), SizedBox 10x10x4))
    // → "Center 100x100x50 @ 0,0,0\n  Padding 10x10x12 @ 45,45,19\n    SizedBox 10x10x4 @ 0,0,3\n"
}
#[test]
fn padding_in_a_zero_size_window() {
    assert_eq!(layout_of(Size::ZERO, rc(Padding::new(EdgeInsets::all(10.0), sized(5.0, 5.0)))),
        "Padding 0x0x0 @ 0,0,0\n  SizedBox 0x0x0 @ 10,10,0\n");
}

// align.rs
#[test]
fn align_positions_child() { /* janela 100x50: Align(BOTTOM_RIGHT, sized(20,10)) → filho @ 80,40,0 */ }
#[test]
fn align_factors_size_to_child() {
    // Center(Align(TOP_LEFT, sized(10,10)).width_factor(2).height_factor(3))
    // → "  Align 20x30x0 @ 40,35,0\n    SizedBox 10x10x0 @ 0,0,0"
}

// aspect_ratio.rs
#[test]
fn aspect_ratio_uses_max_width() {
    assert_eq!(layout_of(W, rc(Center::new(rc(AspectRatio::new(2.0, rc(SizedBox::new())))))),
        "Center 100x100x0 @ 0,0,0\n  AspectRatio 100x50x0 @ 0,25,0\n    SizedBox 100x50x0 @ 0,0,0\n");
}
#[test]
fn aspect_ratio_limited_by_height() { /* razão 0.5 → AspectRatio 50x100x0 @ 25,0,0 */ }
#[cfg(debug_assertions)]
#[test]
#[should_panic(expected = "AspectRatio precisa de largura ou altura limitada")]
fn aspect_ratio_without_bounds_panics_in_debug() {
    // LayoutProbe root com child_constraints(Constraints::unbounded()) → AspectRatio
}
#[cfg(not(debug_assertions))]
#[test]
fn aspect_ratio_without_bounds_takes_smallest_in_release() { /* AspectRatio 0x0x0 */ }
```

- [ ] **Passo 2: rodar e ver falhar.** `cargo test -p notify-rs --lib widgets`: erro de compilação.
- [ ] **Passo 3: implementar** os quatro arquivos e o `mod.rs`.
- [ ] **Passo 4: rodar.**
  - `cargo test -p notify-rs --lib widgets`;
  - `cargo test -p notify-rs --lib --release widgets::aspect_ratio`;
  - clippy e fmt.
- [ ] **Passo 5: commit.** `git commit -m "Adiciona ConstrainedBox, SizedBox, Padding, Align, Center e AspectRatio"`

---

### Task 6: `Flex`, `Row`, `Column`, `Expanded`, `Flexible`, `Spacer`

**Arquivos:**
- Criar: `notify-rs/src/widgets/flex.rs`
- Modificar: `widgets/mod.rs`

**Interfaces:**
- Enums públicos (`Clone + Copy + Debug + PartialEq + Eq`):
  - `MainAxisAlignment { Start, End, Center, SpaceBetween, SpaceAround, SpaceEvenly }`;
  - `CrossAxisAlignment { Start, End, Center, Stretch }`;
  - `MainAxisSize { Max, Min }`;
  - `FlexFit { Tight, Loose }`.
- **`pub struct FlexParentData { pub flex: u32, pub fit: FlexFit }`.**
- **`Flex::new(Axis, children)`, `Row::new(children)` e `Column::new(children)`**, com `.main_axis_alignment`, `.cross_axis_alignment`, `.main_axis_size` e `.spacing` (padrões do spec §5.4).
  - Os três guardam um `FlexConfig { axis, main_alignment, cross_alignment, main_size, spacing }`.
  - Os elements chamam `fn flex_layout(core: &mut ElementCore, config: &FlexConfig, c: Constraints) -> Size`.
- **`Expanded::new(child)` e `Flexible::new(child)`**, com `.flex(u32)` (padrão 1), e **`Spacer::new()`**, com `.flex(u32)`. Os três têm `parent_data()` → `FlexParentData`.
- O algoritmo e as mensagens de layout impossível estão no spec §§5.4 e 6.

- [ ] **Passo 1: escrever os testes que falham** (`flex.rs`). `W = Size::flat(300.0, 100.0)`, e `xs(app)` lê o `offset().x` dos filhos da raiz.

```rust
#[test]
fn row_places_children_from_start() {
    assert_eq!(layout_of(W, rc(Row::new(vec![sized(50.0, 20.0), sized(30.0, 40.0)]))),
        "Row 300x100x0 @ 0,0,0\n  SizedBox 50x20x0 @ 0,40,0\n  SizedBox 30x40x0 @ 50,30,0\n");
}

#[test]
fn main_axis_alignments() {
    // três sized(50,20), sobra 150; cada alinhamento → xs:
    // Start [0,50,100]; End [150,200,250]; Center [75,125,175]
    // SpaceBetween [0,125,250]; SpaceAround [25,125,225]; SpaceEvenly [37.5,125,212.5]
}

#[test]
fn cross_axis_alignments() {
    // Row[sized(50,20)]: Start → y 0; End → y 80; Center → y 40; Stretch → filho 50x100 em y 0
}

#[test]
fn main_axis_size_min_shrinks() { /* Center(Row[50x20, 30x40].main_axis_size(Min)) → "  Row 80x40x0 @ 110,30,0" */ }

#[test]
fn spacing_adds_gaps() { /* três sized(50,20) com spacing 10 → xs [0,60,120] */ }

#[test]
fn expanded_takes_remaining_space() {
    // Row[sized(50,20), Expanded(sized(10,20)), sized(50,20)] → Expanded 200x20x0 @ 50,40,0; xs [0,50,250]
}

#[test]
fn flex_factors_split_space() { /* Row[Expanded(SizedBox::new()), Expanded(SizedBox::new()).flex(2)] → larguras 100 e 200 */ }

#[test]
fn flexible_is_loose() { /* Row[Flexible(sized(50,20)), sized(50,20)] → xs [0,50] */ }

#[test]
fn spacer_pushes_children_apart() { /* Row[sized(50,20), Spacer::new(), sized(50,20)] → xs [0,50,250]; Spacer 200x0x0 */ }

#[test]
fn children_overflow_from_start() { /* Row[sized(200,20), sized(200,20)].main_axis_alignment(Center) → xs [0,200], Row 300 */ }

#[test]
fn column_lays_out_vertically() { /* Column[50x20, 30x40] → filhos @ 125,0,0 e 135,20,0 */ }

#[test]
fn flex_along_z_stacks_back_to_front() {
    // volume Size::new(100,100,90); Flex::new(Axis::Z, [SizedBox 10x10x10, SizedBox 10x10x20])
    // → Flex 100x100x90; filhos @ 45,45,0 e 45,45,10
}

#[test]
fn center_inside_row_takes_child_width() {
    // Row[Center(sized(10,10))] → "  Center 10x100x0 @ 0,0,0\n    SizedBox 10x10x0 @ 0,45,0"
}

#[test]
fn expanded_outside_flex_passes_through() {
    // janela 100x100: Center(Expanded(sized(10,10))) → "  Expanded 10x10x0 @ 45,45,0"
}

#[test]
fn changing_flex_factor_changes_sizes() {
    // set_root(Row[Expanded(probe a), Expanded(probe b)]) → larguras 150/150
    // set_root(Row[Expanded(probe a).flex(2), Expanded(probe b)]) → 200/100
}

#[test]
fn inserted_child_is_laid_out_in_the_same_update() {
    // Probe da parte 1 (key + filho, layout padrão = tamanho do filho):
    // Row[Probe a#1 (filho sized 50x20), Probe b#2 (filho sized 50x20)]
    // → set_root(Row[a#1, Probe c#3 (filho sized 30x20), b#2]) → xs [0,50,80]; c com 30x20
}

#[test]
fn reordered_keyed_children_get_new_offsets() {
    // Row[Probe a#1 (filho sized 50x20), Probe b#2 (filho sized 30x20)] → Row[b#2, a#1]
    // → xs [0,30]; o log não ganha nenhum "create" depois do primeiro set_root
}

#[cfg(debug_assertions)]
#[test]
#[should_panic(expected = "Flex com filhos flexíveis precisa de eixo principal limitado")]
fn flex_children_need_bounded_main_axis() { /* LayoutProbe unbounded → Row[Expanded(sized(10,10))] */ }

#[cfg(not(debug_assertions))]
#[test]
fn flex_children_get_zero_without_bounded_main_axis() { /* Expanded 0 de largura */ }

#[cfg(debug_assertions)]
#[test]
#[should_panic(expected = "Flex com Stretch precisa de eixos transversais limitados")]
fn stretch_needs_bounded_cross_axes() { /* LayoutProbe unbounded → Row[sized].cross(Stretch) */ }

#[cfg(not(debug_assertions))]
#[test]
fn stretch_falls_back_to_start_without_bounds() { /* filho em y 0 com a própria altura */ }
```

- [ ] **Passo 2: rodar e ver falhar.** `cargo test -p notify-rs --lib widgets::flex`: erro de compilação.
- [ ] **Passo 3: implementar** `flex.rs` conforme o spec §5.4.
- [ ] **Passo 4: rodar.**
  - `cargo test -p notify-rs --lib widgets::flex`;
  - `cargo test -p notify-rs --lib --release widgets::flex`;
  - clippy e fmt.
- [ ] **Passo 5: commit.** `git commit -m "Adiciona Flex, Row, Column, Expanded, Flexible e Spacer"`

---

### Task 7: `Stack` e `Positioned`

**Arquivos:**
- Criar: `notify-rs/src/widgets/stack.rs`
- Modificar: `widgets/mod.rs`

**Interfaces:**
- **`StackFit { Loose, Expand, Passthrough }`.**
- **`pub struct PositionedParentData`**, com os campos públicos `left`, `top`, `right`, `bottom`, `back`, `front`, `width`, `height` e `depth`, todos `Option<f32>`.
- **`Stack::new(children)`**, com `.alignment(Alignment)` (padrão `TOP_LEFT`) e `.fit(StackFit)` (padrão `Loose`).
- **`Positioned::new(child)`**, com os nove encadeados de mesmo nome.
- O algoritmo está no spec §5.5.

- [ ] **Passo 1: escrever os testes que falham** (`stack.rs`). `W = Size::flat(300.0, 100.0)`.

```rust
#[test]
fn stack_sizes_to_largest_non_positioned_child() {
    // Center(Stack[sized(50,20), sized(30,40)]) → "  Stack 50x40x0 @ 125,30,0", filhos @ 0,0,0
}
#[test]
fn stack_alignment_positions_children() { /* .alignment(CENTER) → filhos @ 0,10,0 e 10,0,0 */ }
#[test]
fn stack_fit_expand_makes_children_fill() { /* Stack raiz .fit(Expand)[sized(50,20)] → filho 300x100 */ }
#[test]
fn stack_without_non_positioned_children_takes_biggest() { /* Center(Stack[Positioned(sized(10,10)).left(0)]) → Stack 300x100 */ }
#[test]
fn positioned_with_insets_and_sizes() {
    // Stack raiz [sized(10,10), Positioned(sized(1,1)).left(10).right(20).top(5).height(30),
    //             Positioned(sized(20,20)).right(10).bottom(10)]
    // → segundo filho 270x30 @ 10,5,0; terceiro 20x20 @ 270,70,0
}
#[test]
fn positioned_in_depth() {
    // volume Size::new(100,100,50): Stack raiz [Positioned(SizedBox 10x10x10).back(5), Positioned(idem).front(5)]
    // → z 5 e z 35
}
#[test]
fn positioned_outside_stack_passes_through() { /* janela 100x100: Center(Positioned(sized(10,10)).left(3)) → @ 45,45,0 */ }
#[cfg(debug_assertions)]
#[test]
#[should_panic(expected = "Stack com StackFit::Expand precisa de eixos limitados")]
fn stack_expand_needs_bounds() { /* LayoutProbe unbounded → Stack.fit(Expand)[sized(10,10)] */ }
#[cfg(not(debug_assertions))]
#[test]
fn stack_expand_without_bounds_is_loose_in_release() { /* filho 10x10 */ }
```

- [ ] **Passo 2: rodar e ver falhar.** `cargo test -p notify-rs --lib widgets::stack`: erro de compilação.
- [ ] **Passo 3: implementar** conforme o spec §5.5.
- [ ] **Passo 4: rodar.** Debug e release no filtro `widgets::stack`. Rodar também clippy e fmt.
- [ ] **Passo 5: commit.** `git commit -m "Adiciona Stack e Positioned"`

---

### Task 8: `Wrap`

**Arquivos:**
- Criar: `notify-rs/src/widgets/wrap.rs`
- Modificar: `widgets/mod.rs`

**Interfaces:**
- **`WrapAlignment`**, com os seis valores de `MainAxisAlignment`, e **`WrapCrossAlignment { Start, End, Center }`**.
- **`Wrap::new(children)`**, com `.direction(Axis)` (padrão X), `.spacing`, `.run_spacing`, `.alignment`, `.run_alignment` e `.cross_axis_alignment` (padrões `Start`).
- O algoritmo está no spec §5.7. A distribuição de sobra é a mesma do `Flex`, então extraia uma função compartilhada `distribute(alignment, sobra, n, spacing) -> (início, entre)` em `widgets/flex.rs`, com visibilidade `pub(crate)`.

- [ ] **Passo 1: escrever os testes que falham** (`wrap.rs`). `W = Size::flat(100.0, 100.0)`, com 3 filhos `sized(40,10)` salvo indicação.

```rust
#[test]
fn wrap_breaks_lines() { /* Wrap raiz → filhos @ (0,0) (40,0) (0,10) */ }
#[test]
fn wrap_spacing_and_run_spacing() { /* .spacing(10).run_spacing(5) → (0,0) (50,0) (0,15) */ }
#[test]
fn wrap_alignment_centers_each_run() { /* .alignment(Center) → x 10 e 50 na 1ª linha; x 30 na 2ª */ }
#[test]
fn wrap_run_alignment_end() { /* .run_alignment(End) → y 80 e 90 */ }
#[test]
fn wrap_cross_alignment_inside_run() { /* filhos sized(40,10) e sized(40,30) com .cross_axis_alignment(Center) → y 10 e 0 */ }
#[test]
fn wrap_shrinks_to_content() { /* Center(Wrap) → "  Wrap 80x20x0 @ 10,40,0" */ }
#[test]
fn wrap_vertical_direction() { /* janela 100x25, .direction(Axis::Y), 3 filhos sized(10,10) → (0,0) (0,10) (10,0) */ }
#[cfg(debug_assertions)]
#[test]
#[should_panic(expected = "Wrap só aceita Axis::X ou Axis::Y")]
fn wrap_along_z_panics_in_debug() {}
#[cfg(not(debug_assertions))]
#[test]
fn wrap_along_z_uses_x_in_release() { /* igual a wrap_breaks_lines */ }
```

- [ ] **Passo 2: rodar e ver falhar.** `cargo test -p notify-rs --lib widgets::wrap`: erro de compilação.
- [ ] **Passo 3: implementar** conforme o spec §5.7, com `distribute` compartilhado. Os testes do `Flex` devem continuar passando.
- [ ] **Passo 4: rodar.** Debug e release nos filtros `widgets::wrap` e `widgets::flex`. Rodar também clippy e fmt.
- [ ] **Passo 5: commit.** `git commit -m "Adiciona Wrap"`

---

### Task 9: `Grid`

**Arquivos:**
- Criar: `notify-rs/src/widgets/grid.rs`
- Modificar: `widgets/mod.rs`

**Interfaces:**
- **`GridRowHeight { AspectRatio(f32), Content }`.**
- **`Grid::new(columns: usize, children)`**, com `.row_spacing(f32)`, `.column_spacing(f32)` e `.row_height(GridRowHeight)` (padrão `AspectRatio(1.0)`).
- O algoritmo está no spec §5.8.

- [ ] **Passo 1: escrever os testes que falham** (`grid.rs`). `W = Size::flat(210.0, 300.0)`.

```rust
#[test]
fn grid_square_cells() {
    // Center(Grid::new(2, 3×SizedBox::new()).column_spacing(10).row_spacing(10))
    // → "  Grid 210x210x0 @ 0,45,0"; filhos 100x100 @ (0,0) (110,0) (0,110)
}
#[test]
fn grid_aspect_ratio_cells() { /* .row_height(AspectRatio(2.0)) → filhos 100x50, terceiro @ 0,60 (com row_spacing 10) */ }
#[test]
fn grid_content_row_height() {
    // .row_height(Content), filhos SizedBox::new().height(30/50/20), row_spacing 10
    // → filhos 100x30 @ (0,0), 100x50 @ (110,0), 100x20 @ (0,60)
}
#[test]
fn grid_without_children_is_flat() { /* Center(Grid::new(3, vec![])) → Grid 210x0x0 */ }
#[cfg(debug_assertions)]
#[test]
#[should_panic(expected = "Grid precisa de largura limitada")]
fn grid_needs_bounded_width() { /* LayoutProbe unbounded → Grid */ }
#[cfg(debug_assertions)]
#[test]
#[should_panic(expected = "Grid precisa de pelo menos uma coluna")]
fn grid_needs_columns() {}
#[cfg(not(debug_assertions))]
#[test]
fn impossible_grid_is_smallest_with_zero_children() { /* os dois casos → Grid 0x0x0, filhos 0x0x0 */ }
```

- [ ] **Passo 2: rodar e ver falhar.** `cargo test -p notify-rs --lib widgets::grid`: erro de compilação.
- [ ] **Passo 3: implementar** conforme o spec §5.8.
- [ ] **Passo 4: rodar.** Debug e release no filtro `widgets::grid`. Rodar também clippy e fmt.
- [ ] **Passo 5: commit.** `git commit -m "Adiciona Grid de colunas fixas"`

---

### Task 10: layout visto de fora e verificação final

**Arquivos:**
- Criar: `notify-rs/tests/layout_api.rs`
- Modificar: `notify-rs/src/lib.rs`

**Interfaces:**
- `lib.rs` reexporta:
  - a geometria;
  - `impossible_layout`;
  - todos os widgets;
  - os enums `MainAxisAlignment`, `CrossAxisAlignment`, `MainAxisSize`, `FlexFit`, `StackFit`, `WrapAlignment`, `WrapCrossAlignment` e `GridRowHeight`;
  - `FlexParentData` e `PositionedParentData`.
- A documentação da crate (`//!`) ganha um parágrafo sobre layout: `App::set_size`, restrições e o gancho `Element::layout`.

- [ ] **Passo 1: escrever o teste de integração** (`tests/layout_api.rs`), usando só `notify_rs::*`:
  - **`Diagonal`:** um `Widget` com element próprio. O `layout` dele faz o layout de cada filho com `constraints.loosen()` e posiciona cada um no fim do anterior (`x += largura`, `y += altura`). O tamanho é `constraints.constrain(soma)`.
  - **`custom_layout_element_positions_children`:** janela 100x100, `Diagonal[SizedBox 10x20, SizedBox 30x5]` → filhos em `0,0,0` e `10,20,0`, e `Diagonal` com 100x100 (restrições exatas da janela). Confirmar também pelo `debug_layout`.
  - **`custom_element_reads_flex_parent_data`:** um element próprio que soma os `FlexParentData::flex` dos filhos, lidos por `child.parent_data().and_then(|d| d.downcast_ref::<FlexParentData>())`, e devolve uma largura igual a essa soma. Com filhos `Expanded::new(...).flex(2)` e `Spacer::new().flex(3)`, a largura é 5.
- [ ] **Passo 2: rodar.** `cargo test -p notify-rs --test layout_api`. Ajustar as reexportações até passar.
- [ ] **Passo 3: verificação final**

Rodar: `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && cargo test --workspace --release && cargo doc --workspace --no-deps`
Esperado: tudo passando, sem avisos, nos dois perfis.

- [ ] **Passo 4: commit.** `git commit -m "Expõe a API de layout e testa um layout próprio externo"`

---

## Cobertura do spec

| Seção do spec | Tarefa |
|---|---|
| §2 geometria | 1 |
| §3.1–3.2 ganchos do `Element` e `ElementCore` | 3 (`needs_layout_listener` é decisão deste plano) |
| §3.3 `App::set_size`, `update`, `debug_layout` | 3 |
| §3.4 `impossible_layout` | 3 |
| §4.1–4.3 estado, fronteiras e marcação | 2, 3 (fronteira), 4 (quem marca) |
| §4.4–4.5 passada de layout e garantias | 3 |
| §5.1–5.3, 5.6 widgets de um filho | 5 |
| §5.4 família `Flex` | 6 |
| §5.5 `Stack`/`Positioned` | 7 |
| §5.7 `Wrap` | 8 |
| §5.8 `Grid` | 9 |
| §5.9 dados de pai fora do lugar | 6, 7 |
| §6 validação e erros | 3 (restrições e tamanho), 5–9 (casos impossíveis) |
| §8 testes (geometria, widgets, incremental, erros, integração) | 1, 3, 4, 5–9, 10 |
