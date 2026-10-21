# notify: núcleo (parte 1), design

- **Data:** 2026-10-21
- **Status:** aprovado em conversa, aguardando revisão deste documento
- **Crate:** `notify-rs` (em `jr-rs/notify-rs`)

## 1. Contexto e objetivo

`notify` é uma biblioteca genérica de interface gráfica no estilo Flutter
(widgets descrevem a interface, elements são as instâncias vivas na árvore).
Outras bibliotecas a usam como base. A primeira prevista é `opengl-notify-rs`,
um backend que desenha com OpenGL.

O objetivo é servir de base para **aplicações reais**: robustez, comportamento
previsível e uma API pública que outras bibliotecas possam estender.

### Fronteira entre `notify` e backends

O `notify` cuida de widgets, elements, layout, hit testing e despacho de
eventos, e pinta numa lista abstrata de comandos de desenho. O backend cria a
janela, entrega eventos ao `notify` e rasteriza os comandos.

### Divisão em partes

O framework é dividido em partes, cada uma com seu próprio ciclo de spec,
plano e implementação. **Este documento cobre só a parte 1.**

| # | Parte | Conteúdo | Depende de |
|---|---|---|---|
| 1 | **Núcleo** | widgets, elements, reconciliação, keys, reconstrução agendada, dados compartilhados (`provide`/`get`), integração com `reactive-rs` | — |
| 2 | Render e layout | objetos de render, restrições, protocolo de layout, `Row`/`Column`/`Padding`/`Center`/`Stack` | 1 |
| 3 | Pintura | lista de comandos de desenho, canvas, recorte e transformações | 2 |
| 4 | Texto | fontes, shaping, quebra de linha e medição (provavelmente com `cosmic-text`) | 2, 3 |
| 5 | Backend e frames | trait de backend, loop de frames, backend headless para testes | 1–3 |
| 6 | Entrada | eventos de ponteiro e teclado, hit testing, gestos, foco | 2, 5 |
| 7 | `opengl-notify-rs` | primeiro backend real, em projeto separado | 5 |

## 2. Escopo da parte 1

**Dentro:**
- os traits `Widget`, `Element` e `BuilderWidget`;
- `Key`, `BuildContext`, `RebuildHandle` e `BuildOwner`;
- a reconciliação de filhos, inclusive listas com key;
- a reconstrução agendada, com notificações vindas de qualquer thread;
- os dados compartilhados pela árvore via `BuildContext::provide`/`get`;
- a mudança no `reactive-rs` descrita na seção 5;
- o workspace Cargo em `jr-rs/`.

**Fora:**
- qualquer widget visual (`Text`, `Row`, `Button`...);
- render, layout, pintura, texto, entrada e backend (partes 2 a 7);
- `GlobalKey` (mover uma subárvore mantendo o estado);
- widgets assíncronos (`FutureBuilder`);
- widget de erro no lugar de uma subárvore que entrou em pânico.

## 3. Decisões tomadas

| Decisão | Escolha | Alternativas descartadas e por quê |
|---|---|---|
| Armazenamento da árvore | Arena `SlotMap<ElementId, ElementNode>` dentro do `BuildOwner` | `Rc<RefCell<Element>>`: reentrância vira `BorrowMutError` em tempo de execução, `Rc` não é `Send` e há risco de ciclos. Reconstruir tudo e comparar: custo O(n) por mudança e estado frágil, preso ao caminho na árvore. |
| Estado | **Sem estado local.** Widgets guardam dados e um `Notifier` em campos (via `Arc`) criados fora da árvore. O element assina o notifier e reconstrói quando ele avisa. | `StatefulWidget` + `State` com `set_state` enfileirado: mais conceitos e uma fila de closures. Estado criado por `create_state` e guardado pelo element: recusado em favor do modelo acima. |
| Dados compartilhados | `ctx.provide(valor)` e `ctx.get::<T>()` no `BuildContext` | Um widget `InheritedWidget` ou `Inherited<T>` na árvore. |
| Tipo comum dos widgets | `Rc<dyn Widget>` | Um `AnyWidget` próprio com `.into_widget()` e trait marcador. Desnecessário com um único trait base. |
| Extensibilidade | `Widget::create_element` e `Element` **públicos**, para outras bibliotecas criarem tipos de element | Tipos de element internos ao crate: impediriam `opengl-notify-rs` e terceiros de criar widgets com elements próprios. |
| Onde fica o widget atual | No **nó da arena**, gerenciado pelo framework | No próprio element: cada implementação teria de guardar e atualizar o widget, e um esquecimento quebraria a reconciliação sem erro de compilação. |
| Dependências externas | Crates consolidadas são permitidas, com justificativa | Zero dependências. |

## 4. Estrutura do repositório

`jr-rs/` passa a ser um workspace Cargo:

```toml
# jr-rs/Cargo.toml
[workspace]
members = ["reactive-rs", "notify-rs"]
resolver = "3"
```

- **`.gitignore` da raiz:** `/target` e `.idea/`. Com o workspace, `target/`
  e `Cargo.lock` ficam na raiz. Os `Cargo.lock` dos membros deixam de ser
  usados e são removidos.
- **`notify-rs/Cargo.toml`:**

  ```toml
  [package]
  name = "notify-rs"
  version = "0.1.0"
  edition = "2024"
  license = "MIT"

  [dependencies]
  reactive-rs = { path = "../reactive-rs" }
  slotmap = "1"
  ```

  `slotmap` fornece uma arena com ids geracionais: um `ElementId` de um
  element já removido é detectado em vez de apontar para outro element.
- **Módulos de `notify-rs/src/`:**

  | Arquivo | Conteúdo |
  |---|---|
  | `lib.rs` | declara os módulos e reexporta a API pública |
  | `widget.rs` | `Widget`, `BuilderWidget`, `impl<B: BuilderWidget> Widget for B` |
  | `builder_element.rs` | `BuilderElement<B>`, interno |
  | `element.rs` | trait `Element` |
  | `key.rs` | `Key` |
  | `context.rs` | `BuildContext` e `RebuildHandle` |
  | `owner.rs` | `BuildOwner`: arena, passada de reconstrução, montagem e desmontagem, `provide`/`get` |
  | `reconcile.rs` | algoritmo de reconciliação de listas de filhos |
  | `inbox.rs` | caixa de entrada thread safe de ids sujos, interna |
  | `test_support.rs` | widgets e elements de teste (só `cfg(test)`) |

- O esqueleto atual de `notify-rs/src/lib.rs` é substituído por completo.

## 5. Mudança no `reactive-rs`

`ValueListenable<T>` ganha um método para ler o valor atual:

```rust
pub trait ValueListenable<T>: Send + Sync {
    fn get(&self) -> T;
    fn add_listener(&self, listener: ValueListener<T>) -> usize;
    fn remove_listener(&self, id: usize);
    // subscribe: sem mudança
}

impl<T: Clone + Send> ValueListenable<T> for ValueNotifier<T> {
    fn get(&self) -> T { /* mesmo valor do ValueNotifier::get inerente */ }
    // ...
}
```

O `impl` passa de `T: Send` para `T: Clone + Send`. Nada útil se perde,
porque `set` e `update` já exigem `Clone`. O README do `reactive-rs` menciona
o novo método, e um teste cobre `get` chamado através de
`&dyn ValueListenable<T>`.

## 6. API pública do `notify-rs`

### 6.1 `Widget`

```rust
pub trait Widget: Any {
    fn key(&self) -> Option<Key> { None }
    fn create_element(&self) -> Box<dyn Element>;

    #[doc(hidden)]
    fn type_name(&self) -> &'static str { std::any::type_name::<Self>() }
}
```

- Widgets são configurações imutáveis, passadas como `Rc<dyn Widget>`.
- `Rc` (e não `Box`) permite que pai e element compartilhem o mesmo widget,
  torna barato clonar, e permite a regra "mesma instância → pula a subárvore"
  com `Rc::ptr_eq`. `Arc` não é necessário, porque a árvore vive numa thread
  só.
- `Widget: Any` dá ao framework o `TypeId` do tipo concreto e permite
  converter `Rc<dyn Widget>` de volta para `Rc<W>`.
- `type_name` serve para o `debug_tree` e não deve ser sobrescrito.

### 6.2 `Element`

```rust
pub trait Element {
    fn mount(&mut self, ctx: &mut BuildContext<'_>) {}
    fn update(&mut self, old_widget: &Rc<dyn Widget>, ctx: &mut BuildContext<'_>) {}
    fn build(&mut self, ctx: &mut BuildContext<'_>) -> Vec<Rc<dyn Widget>>;
    fn unmount(&mut self, ctx: &mut BuildContext<'_>) {}
}
```

Contrato:
- **`mount`:** chamado uma vez, logo depois de o element entrar na árvore e
  antes do primeiro `build`. `ctx.widget()` é o widget que o criou.
- **`update`:** chamado quando o pai entrega uma instância diferente de widget
  com o mesmo tipo e a mesma key. `ctx.widget()` já é o widget novo e
  `old_widget` é o anterior. É sempre seguido de `build`.
- **`build`:** devolve os widgets filhos, em ordem. É chamado depois de
  `mount`, depois de `update` e quando o element foi marcado como sujo. O
  `BuildOwner` reconcilia a lista devolvida (seção 7.3). Quem implementa um
  element não reconcilia nada.
- **`unmount`:** chamado uma vez, depois que todos os filhos já foram
  desmontados. `ctx.widget()` continua válido.
- **Recursos:** recursos que precisam ser liberados, como assinaturas de
  notifiers, devem ser liberados no `Drop` do element. Assim eles são
  liberados também quando o `BuildOwner` é destruído envenenado (seção 10).
  `unmount` é para lógica que precisa do contexto.
- **Métodos novos:** a parte 2 vai acrescentar ganchos de render e layout
  como métodos com implementação padrão. Isso não quebra quem já implementa
  `Element`.

### 6.3 `BuilderWidget`

```rust
pub trait BuilderWidget: Any {
    fn key(&self) -> Option<Key> { None }
    fn notifier(&self) -> Option<&Arc<Notifier>> { None }
    fn build(&self, ctx: &mut BuildContext<'_>) -> Rc<dyn Widget>;

    /// Roda `change` e depois chama `notify_listeners()` no notifier.
    /// Entra em pânico se o widget não tiver notifier.
    fn set(&self, change: impl FnOnce()) where Self: Sized { /* ... */ }
}

impl<B: BuilderWidget> Widget for B {
    fn key(&self) -> Option<Key> { BuilderWidget::key(self) }
    fn create_element(&self) -> Box<dyn Element> { Box::new(BuilderElement::<B>::new()) }
}
```

- `BuilderWidget` é o trait que quem usa o framework implementa no dia a dia.
  Sem notifier, é um widget composto sem estado. Com notifier, ele é
  reconstruído sempre que o notifier avisa.
- `Notifier` é o `reactive_rs::Notifier`.
- O compilador confirmou que uma crate de fora pode implementar `Widget`
  direto para um tipo próprio, apesar do impl genérico, desde que o tipo não
  implemente `BuilderWidget`.

**`BuilderElement<B>`** (interno):

| Momento | O que faz |
|---|---|
| `mount` | Se `widget.notifier()` for `Some(n)`, chama `n.add_listener` com um listener que chama `rebuild_handle.mark_needs_build()`, e guarda `(Arc<Notifier>, id)`. |
| `update` | Se o notifier do widget novo não for o mesmo `Arc` (`Arc::ptr_eq`), ou se um dos dois for `None`, remove o listener antigo e assina o novo. |
| `build` | Devolve `vec![ctx.widget_as::<B>().build(ctx)]`. |
| `Drop` | Remove o listener, se houver. Não implementa `unmount`. |

**Onde mora o dado.** Dados e notifier ficam em campos do widget, via `Arc`,
e precisam ser criados por quem vive mais que o widget: o topo da aplicação
ou um modelo. Um pai que cria `Arc::new(Notifier::new())` dentro do próprio
`build` recria o notifier a cada reconstrução. A documentação de
`BuilderWidget` explica isso.

Exemplo (`Button` e `Text` são da parte 2 em diante, aqui só para ilustrar):

```rust
#[derive(Clone)]
struct Counter {
    notifier: Arc<Notifier>,
    count: Arc<AtomicI32>,
}

impl BuilderWidget for Counter {
    fn notifier(&self) -> Option<&Arc<Notifier>> { Some(&self.notifier) }

    fn build(&self, _ctx: &mut BuildContext<'_>) -> Rc<dyn Widget> {
        let this = self.clone();
        Rc::new(Button::new(format!("{}", self.count.load(Relaxed)), move || {
            this.set(|| { this.count.fetch_add(1, Relaxed); })
        }))
    }
}
```

### 6.4 `Key`

```rust
#[derive(Clone)]
pub struct Key(/* Rc<dyn KeyValue> */);

impl Key {
    pub fn new<V: Hash + Eq + Debug + 'static>(value: V) -> Key;
}
// PartialEq, Eq, Hash, Debug
// From<&str> e From<String> (guardam String), From<i32>, From<i64>, From<u32>, From<u64>, From<usize>
```

- Duas keys são iguais quando o tipo do valor e o valor são iguais. Por
  exemplo, `Key::new(1u32) != Key::new(1i64)`, e
  `Key::from("a") == Key::new(String::from("a"))`.
- O `Debug` mostra o `Debug` do valor, como `"a"` ou `1`.

### 6.5 `BuildContext` e `RebuildHandle`

```rust
pub struct BuildContext<'a> { /* &'a mut BuildOwner + ElementId */ }

impl BuildContext<'_> {
    pub fn widget(&self) -> Rc<dyn Widget>;
    pub fn widget_as<W: Widget>(&self) -> Rc<W>;
    pub fn rebuild_handle(&self) -> RebuildHandle;
    pub fn provide<T: PartialEq + 'static>(&mut self, value: T);
    pub fn get<T: 'static>(&mut self) -> Option<Rc<T>>;
}

#[derive(Clone)]
pub struct RebuildHandle { /* ElementId + Arc<Inbox> */ }   // Send + Sync

impl RebuildHandle {
    pub fn mark_needs_build(&self);
}
```

- **`widget_as::<W>()`** devolve o widget atual já com o tipo concreto. Entra
  em pânico se o tipo não for `W`. Isso nunca acontece para o próprio tipo de
  um element, porque o `BuildOwner` só reaproveita um element quando o tipo
  do widget é o mesmo.
- **`rebuild_handle()`** pode ser chamado a partir de qualquer thread. Se o
  element já saiu da árvore, `mark_needs_build` não faz nada.
- **`provide` e `get`** só podem ser chamados durante o `Element::build`
  deste element. Chamados em `mount`, `update` ou `unmount`, causam pânico
  com mensagem clara. As regras completas estão na seção 8.

### 6.6 `BuildOwner`

```rust
pub struct BuildOwner { /* ... */ }

impl BuildOwner {
    pub fn new() -> Self;
    pub fn set_root(&mut self, widget: Rc<dyn Widget>);
    pub fn on_needs_build(&mut self, wake: impl Fn() + Send + Sync + 'static);
    pub fn needs_build(&self) -> bool;
    pub fn build(&mut self);
    pub fn debug_tree(&self) -> String;
}
// Default e Drop (seção 10)
```

- **`set_root`:** na primeira chamada, monta a árvore inteira, de forma
  síncrona. Nas seguintes, aplica as regras de atualização de filho (seção
  7.2) à raiz. Mudanças pendentes na caixa de entrada não são processadas
  aqui, só em `build()`.
- **`on_needs_build`:** registra o callback que pede um frame ao backend,
  substituindo o anterior. Se já houver mudanças pendentes no momento do
  registro, o callback novo é chamado uma vez.
- **`needs_build`:** é verdadeiro quando a caixa de entrada não está vazia.
- **`build`:** processa as mudanças pendentes (seção 9).
- **`debug_tree`:** devolve uma linha por element, em pré-ordem, com dois
  espaços de recuo por nível de profundidade. Cada linha tem o nome curto do
  tipo do widget e, se houver key, ` #` seguido do `Debug` da key. O nome
  curto é o `type_name` sem os caminhos de módulo de cada segmento. Exemplo:

  ```
  App
    Group
      Item #"a"
      Item #"b"
  ```

## 7. Árvore de elements

### 7.1 Nó da arena

```rust
struct ElementNode {
    widget: Rc<dyn Widget>,
    element: Option<Box<dyn Element>>,   // None só enquanto o element está retirado para uma chamada
    parent: Option<ElementId>,
    children: Vec<ElementId>,
    depth: usize,                        // raiz = 0
    dirty: bool,
    provided: HashMap<TypeId, Provided>, // valores do último provide (seção 8)
    dependents: HashMap<TypeId, HashSet<ElementId>>,  // quem leu cada tipo deste nó
    dependencies: HashMap<TypeId, Option<ElementId>>, // o que este nó leu, e de quem
}
```

Para chamar um método do element, o `BuildOwner` o retira do nó
(`Option::take`), cria o `BuildContext` e chama o método. Depois devolve o
element ao nó. Isso permite que o element receba `&mut BuildOwner` via
contexto sem empréstimo duplo.

### 7.2 Montar, atualizar e desmontar

**`can_update(antigo, novo)`** é verdadeiro quando:
- o `TypeId` do valor concreto é o mesmo (não o `TypeId` do `Rc`); e
- as keys são iguais (ambas `None`, ou iguais).

**Montar (`inflate`) um widget como filho de `parent`:**
1. Cria o nó com o widget, `widget.create_element()`, o pai, a profundidade
   do pai + 1 e `dirty = false`.
2. Chama `element.mount`.
3. Reconstrói o nó (seção 7.4).

**Atualizar o filho (`update_child(antigo, novo)`):**

| Antigo | Novo | Ação |
|---|---|---|
| nenhum | nenhum | nada |
| existe | nenhum | desmonta o antigo |
| nenhum | existe | monta o novo |
| existe | mesma instância (`Rc::ptr_eq`) | nada. A subárvore é pulada; se o nó estava sujo, continua na fila da passada |
| existe | `can_update` | troca o widget do nó, chama `element.update(&antigo, ctx)` e reconstrói o nó |
| existe | outro tipo ou outra key | desmonta o antigo e depois monta o novo |

**Desmontar:**
1. Desmonta os filhos, em ordem.
2. Chama `element.unmount`.
3. Remove as dependências do nó (seção 8).
4. Remove o nó da arena, o que faz o element sofrer `Drop`.

Ids desse nó que ainda estejam em filas ficam inválidos e são ignorados.

### 7.3 Reconciliação de listas (`reconcile.rs`)

`update_children(pai, filhos_antigos, widgets_novos) -> filhos_novos` segue o
algoritmo do Flutter:

1. **Validação:** se duas keys entre os widgets novos forem iguais, entra em
   pânico com a mensagem `key duplicada entre irmãos: {key:?}`.
2. **Topo:** enquanto antigo[i] e novo[i] forem compatíveis (`can_update`),
   aplica `update_child` e avança.
3. **Base:** do fim para o começo, enquanto os pares forem compatíveis, só
   avança, sem atualizar ainda.
4. **Miolo, lado antigo:** os antigos com key vão para um
   `HashMap<Key, ElementId>`, e os sem key são desmontados.
5. **Miolo, lado novo:** para cada widget novo, se ele tem key e o mapa tem
   um element com essa key e tipo compatível, o element sai do mapa e recebe
   `update_child`. Senão, o widget é montado.
6. **Sobras:** os elements que restaram no mapa são desmontados.
7. **Base:** aplica `update_child` aos pares da base, encontrados no passo 3.

### 7.4 Reconstruir um nó

1. Separa o `provided` atual do nó (`provided_antigo`) e limpa as
   dependências do nó (seção 8).
2. Chama `element.build(ctx)`, que devolve os widgets filhos. Durante a
   chamada, `provide` grava em `node.provided` e `get` registra dependências.
3. Compara `provided_antigo` com o novo `provided` e marca os dependentes
   afetados como sujos (seção 8). Isso acontece **antes** da reconciliação,
   para que um dependente reconstruído pela reconciliação não seja
   reconstruído de novo depois.
4. Reconcilia os filhos (seção 7.3).
5. `dirty = false`.

## 8. Dados compartilhados: `provide` e `get`

- **`provide::<T>(valor)`** grava `valor` no nó atual, com o tipo apagado e
  junto com uma função de igualdade de `T`. Se for chamado duas vezes no
  mesmo `build`, vale a última. O valor fica visível **só para os
  descendentes**.
- **`get::<T>()`** sobe a partir do **pai** do nó atual até o primeiro
  ancestral com `T` em `provided`, e devolve `Some(Rc<T>)` ou `None`. Ele
  registra a dependência:
  - em `node.dependencies[T]`, o provedor encontrado ou `None`;
  - em `provedor.dependents[T]`, ou, se nenhum foi encontrado, no índice do
    owner `unresolved[T]`.
  - A busca custa O(profundidade). Um cache por tipo ficaria desatualizado,
    porque `provide` pode mudar a cada `build`.
- **As dependências de um nó refletem exatamente os `get` do último `build`.**
  Elas são limpas no início de cada reconstrução e na desmontagem.
- **Depois do `build` de um provedor P**, para cada tipo T em
  `provided_antigo ∪ provided_novo`:

  | Situação de T | Efeito |
  |---|---|
  | presente antes e depois, valores iguais | nada |
  | presente antes e depois, valores diferentes | marca `P.dependents[T]` como sujos |
  | removido | marca `P.dependents[T]` como sujos. Eles vão resolver para um provedor acima ou para `None` |
  | adicionado | marca como sujos os dependentes de T **dentro da subárvore de P** que estavam resolvidos para um ancestral de P ou em `unresolved[T]`, porque agora o mais próximo é P |

  Para saber se um nó está "dentro da subárvore de P", sobe-se pelos pais a
  partir dele.
- Os dependentes marcados são sempre mais fundos que P. Eles entram na fila
  da passada atual (seção 9) e são processados nela.

## 9. Fluxo de reconstrução e threads

- **Thread única.** `BuildOwner`, `BuildContext`, `Rc<dyn Widget>`, `Key` e
  os elements vivem na thread de interface. O compilador garante isso: por
  conterem `Rc`, não são `Send`.
- **Caixa de entrada (`inbox.rs`).** É um `Arc` com um `Mutex<Vec<ElementId>>`
  de ids pendentes e um `Mutex<Option<Arc<dyn Fn() + Send + Sync>>>` com o
  callback de acordar. Um envenenamento desses mutexes é ignorado, como no
  `reactive-rs`, porque nenhum código de usuário roda enquanto eles estão
  travados.
- **`RebuildHandle::mark_needs_build`** empurra o id. Se a fila estava vazia,
  chama o callback de acordar, fora do lock. O callback é chamado uma vez por
  transição de "vazia" para "não vazia".
- **`BuildOwner::build()`:**
  1. Entra em pânico se o owner estiver envenenado (seção 10).
  2. Troca a fila de pendentes por uma vazia, segurando o lock só para isso.
  3. Para cada id ainda válido na arena: `dirty = true` e insere
     `(profundidade, id)` num `BTreeSet`.
  4. Enquanto o conjunto não estiver vazio, retira o menor par. Se o nó
     existe e está sujo, reconstrói o nó (seção 7.4). Nós marcados durante a
     passada (seção 8) entram no mesmo conjunto.
- **Consequências:**
  - Vários avisos para o mesmo element entre duas chamadas de `build()`
    geram uma reconstrução só.
  - Um element reconstruído como parte de um ancestral fica com
    `dirty = false` e é pulado quando sair do conjunto.
  - Avisos que chegam durante `build()`, inclusive os disparados por código
    de usuário na própria thread, ficam para a próxima chamada, e o backend é
    acordado.
  - Um `set` incondicional dentro de um `build` gera uma reconstrução por
    frame. A documentação de `BuilderWidget` menciona isso.

## 10. Erros

- **Erros de programação** causam pânico com mensagem clara:
  - key duplicada entre irmãos;
  - `widget_as` com o tipo errado;
  - `provide` ou `get` fora do `build`;
  - `BuilderWidget::set` sem notifier.
- **Envenenamento.**
  - `set_root` e `build` marcam o owner como envenenado ao começar e
    desmarcam ao terminar normalmente.
  - Um pânico em código de usuário (`build`, `mount`, `update`, `unmount` ou
    `Drop` de element) deixa o owner envenenado. A partir daí, `set_root` e
    `build` entram em pânico com
    `BuildOwner envenenado por um pânico anterior`.
  - Não há recuperação nesta parte. Um widget de erro no lugar da subárvore
    pode vir a partir da parte 2.
- **`Drop` do `BuildOwner`:**
  - se não estiver envenenado: desmonta a árvore inteira (seção 7.2), dos
    filhos para o pai;
  - se estiver envenenado, o que inclui um `Drop` durante o próprio
    desenrolar do pânico: não chama métodos de `Element`, só destrói a arena.
    Os `Drop` dos elements liberam as assinaturas (seção 6.2).

## 11. Testes

Os testes ficam ao lado de cada módulo, como no `reactive-rs`.
`test_support.rs` fornece:
- um `Log` thread safe que registra eventos;
- um `Leaf` (`Widget` sem filhos);
- um `Group(Vec<Rc<dyn Widget>>)`, um `Widget` com vários filhos e element
  próprio, que exercita a reconciliação de listas;
- um `Probe`, um `Widget` cujo element registra `mount`, `update`, `build`,
  `unmount` e `Drop` no `Log`.

Cenários:

1. **Montagem:** `set_root` constrói a árvore inteira, conferida pelo
   `debug_tree`.
2. **Atualização:**
   - mesmo tipo e mesma key reaproveita o element (`update` + `build`, sem
     `mount`);
   - outro tipo ou outra key desmonta e monta;
   - a mesma instância pula a subárvore inteira.
3. **Listas com key:**
   - reordenar mantém cada element com sua key;
   - inserir e remover no começo, no meio e no fim;
   - filhos sem key no miolo são recriados;
   - key repetida causa pânico.
4. **Ordem do ciclo de vida:** `mount` → `build` na montagem; `update` →
   `build` na atualização; filhos antes do pai na desmontagem; `Drop` depois
   de `unmount`.
5. **`BuilderWidget`:**
   - `set` roda a mudança e notifica;
   - notificação reconstrói só aquele element, no próximo `build()`;
   - notificação vinda de outra thread acorda o backend uma vez por
     transição;
   - vários avisos geram uma reconstrução;
   - trocar de notifier troca a assinatura;
   - desmontar remove o listener (o notifier antigo deixa de acordar o
     owner);
   - `set` sem notifier causa pânico.
6. **`provide`/`get`:**
   - encontra o provedor mais próximo e não enxerga o próprio `provide`;
   - valor igual não reconstrói ninguém, valor diferente reconstrói só os
     dependentes;
   - um provedor que passa a fornecer um tipo no meio da árvore reconstrói
     quem estava resolvido acima ou em `None`;
   - um provedor que deixa de fornecer reconstrói os dependentes;
   - `get` fora do `build` causa pânico;
   - dependências antigas somem quando um `build` deixa de chamar `get`.
7. **Ordem da passada:** pai e filho sujos ao mesmo tempo resultam em um
   `build` de cada; um id de element removido é ignorado.
8. **`on_needs_build`:** registrar com mudanças pendentes chama o callback na
   hora; `needs_build` reflete a fila.
9. **Element personalizado:** um element escrito como outra biblioteca faria
   usa `rebuild_handle` a partir de outra thread e é reconstruído.
10. **Erros:** um pânico em `build` envenena o owner e as chamadas seguintes
    entram em pânico; o `Drop` envenenado não chama `unmount`, mas o `Drop`
    dos elements libera as assinaturas.
11. **Tipos:** `RebuildHandle` é `Send + Sync`.
12. **`reactive-rs`:** `ValueListenable::get` funciona por
    `&dyn ValueListenable<T>`.

## 12. Fora desta parte, para as próximas

- **Parte 2:** ganchos de render e layout no `Element` (como métodos com
  implementação padrão) e os primeiros widgets visuais.
- **A partir da parte 2:** widget de erro no lugar de uma subárvore que entrou
  em pânico.
- **Sem prazo:** `GlobalKey`, widgets assíncronos e uma macro para reduzir o
  código repetitivo de `BuilderWidget`.
- **Nome:** se a crate for publicada no crates.io, avaliar o nome
  `notify-rs`, que pode ser confundido com a crate `notify` (watcher de
  arquivos).
