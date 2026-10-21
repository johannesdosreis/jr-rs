# notify: render e layout (parte 2), design

- **Data:** 2026-10-21
- **Status:** aprovado em conversa, aguardando revisão deste documento
- **Crate:** `notify-rs`
- **Depende de:** parte 1 (`docs/superpowers/specs/2026-10-21-notify-core-design.md`), já implementada

## 1. Contexto e objetivo

A parte 1 entregou a árvore de elements, a reconstrução sob demanda e
`provide`/`get`. A parte 2 faz cada element saber **o próprio tamanho e a
própria posição**. Isso inclui um protocolo de layout, um layout incremental
e os widgets de layout.

O `notify` passa a ter **layout espacial (3D)**: tamanho e posição em x, y e
z. Uma interface plana comum é o caso de profundidade zero.

### Decisões tomadas

| Decisão | Escolha | Descartado e por quê |
|---|---|---|
| Onde mora o layout | No próprio `Element`, com ganchos de implementação padrão. Elements de composição (`BuilderWidget`) são transparentes. | Árvore de render separada, como no Flutter: exigiria sincronizar duas árvores a cada reconciliação. `RenderBox` guardado no core: mais um trait público sem ganho estrutural. |
| Protocolo | O do Flutter: restrições descem, tamanhos sobem, o pai posiciona os filhos. | Flexbox/Grid via `taffy`: outro modelo mental e várias passadas. Solver de restrições: lento e difícil de depurar. |
| Layout incremental | Otimizador completo do Flutter: cache por restrições e fronteiras de relayout (restrições exatas, `parent_uses_size = false`, `sized_by_parent`). | Cache só com restrições exatas: menos fronteiras. Refazer tudo: custo O(n) por mudança. |
| Dimensões | Geometria **3D** desde já; o 2D é o caso de profundidade 0. | Só 2D, ou 3D apenas na pintura: não permitiriam interface espacial. |
| Layout impossível | Pânico em debug e comportamento definido em release. | Sempre degradar: o erro passaria despercebido. Sempre pânico: o app em produção cairia. |
| Widgets | `ConstrainedBox`, `SizedBox`, `Padding`, `Align`, `Center`, `Flex`, `Row`, `Column`, `Expanded`, `Flexible`, `Spacer`, `Stack`, `Positioned`, `AspectRatio`, `Wrap`, `Grid` (colunas fixas) | `Grid` com trilhas (CSS grid) e `Table`: grandes demais para esta parte. |

### Fora desta parte

- Pintura, aviso visual de transbordamento e widget de erro (parte 3).
- Texto (parte 4).
- Backend e loop de frames (parte 5): aqui o tamanho chega por `App::set_size`.
- Hit testing e eventos (parte 6).
- Rolagem, tamanho intrínseco, baseline e direita-para-esquerda.

## 2. Geometria (`geometry.rs`)

Todos os tipos são `Copy + PartialEq + Debug`, usam `f32` e pixels lógicos.

**Convenção:**
- x cresce para a direita, y para baixo e z na direção de quem olha ("mais à frente").
- A posição (`Offset`) de um element é a do seu canto mínimo (esquerda, topo, fundo), relativa ao canto mínimo do pai.

```rust
pub enum Axis { X, Y, Z }

pub struct Size { pub width: f32, pub height: f32, pub depth: f32 }
// Size::new(w, h, d), Size::flat(w, h) (depth 0), Size::ZERO,
// along(axis) -> f32, with_axis(axis, v) -> Size

pub struct Offset { pub x: f32, pub y: f32, pub z: f32 }
// Offset::new(x, y, z), Offset::ZERO, along(axis), with_axis(axis, v)

pub struct Constraints {
    pub min_width: f32, pub max_width: f32,
    pub min_height: f32, pub max_height: f32,
    pub min_depth: f32, pub max_depth: f32,
}
```

`Constraints`:
- **Construtores:**
  - `tight(size)`: mínimo = máximo = size;
  - `loose(size)`: mínimo 0, máximo size;
  - `expand()`: exata infinita nos três eixos, válida só como argumento de `enforce`;
  - `unbounded()`: mínimo 0, máximo infinito.
- **Por eixo:**
  - `min(axis)`, `max(axis)`;
  - `has_bounded(axis)`: o máximo é finito;
  - `with_axis(axis, min, max)`;
  - `tighten(axis, v)`: v limitado ao intervalo do eixo, que vira exato.
- **Globais:**
  - `is_tight()`: mínimo igual ao máximo nos três eixos;
  - `constrain(size)`: limita cada eixo ao seu intervalo;
  - `loosen()`: mínimos viram 0;
  - `enforce(outra)`: limita mínimos e máximos de `self` ao intervalo de `outra`, e o resultado sempre cabe em `outra`;
  - `deflate(insets)`: subtrai os insets de cada eixo; o mínimo não fica negativo, o máximo não fica abaixo do novo mínimo e infinito continua infinito;
  - `biggest()`: `constrain` de um tamanho infinito, ou seja, os máximos;
  - `smallest()`: os mínimos.

**`EdgeInsets { left, top, right, bottom, front, back }`:**
- `ZERO`;
- `all(v)` e `symmetric(horizontal, vertical)`, que afetam só os quatro lados do plano;
- métodos encadeados `left/top/right/bottom(v)` e `with_depth(front, back)`;
- os totais `horizontal()`, `vertical()` e `depth()` (front + back).

**`Alignment { x, y, z }`**, de -1 a 1:
- Constantes do plano, com z = 0, ou seja, centralizadas na profundidade: `TOP_LEFT`, `TOP_CENTER`, `TOP_RIGHT`, `CENTER_LEFT`, `CENTER`, `CENTER_RIGHT`, `BOTTOM_LEFT`, `BOTTOM_CENTER`, `BOTTOM_RIGHT`.
- Constantes de profundidade: `BACK = (0, 0, -1)` e `FRONT = (0, 0, 1)`.
- Métodos: `with_z(z)`, `along(axis)`, e `offset_of(child: Size, parent: Size) -> Offset`, que em cada eixo dá `(parent - child) * (1 + a) / 2`.

## 3. API de layout

### 3.1 `Element`

Os métodos novos têm todos implementação padrão, então nada que implementa
`Element` hoje quebra.

```rust
pub trait Element {
    // core, core_mut, build, widget, children, mark_dirty: parte 1

    /// Faz o layout dos filhos (via `core_mut().layout_child`) e devolve o próprio
    /// tamanho, dentro de `constraints`.
    fn layout(&mut self, constraints: Constraints) -> Size { /* padrão, abaixo */ }

    /// "Meu tamanho só depende das restrições": torna o element fronteira de relayout.
    fn sized_by_parent(&self) -> bool { false }

    /// Dados que o pai lê deste element (flex de `Expanded`, insets de `Positioned`).
    fn parent_data(&self) -> Option<&dyn Any> { None }

    fn size(&self) -> Size { self.core().size() }
    fn offset(&self) -> Offset { self.core().offset() }
}
```

**`layout` padrão:**
- sem filhos: `constraints.smallest()`;
- com filhos: cada filho recebe `constraints` (`parent_uses_size = true`) e fica em `Offset::ZERO`. O tamanho é o maior de cada eixo entre os filhos, passado por `constraints.constrain`.

É isso que `BuilderWidget`, `Expanded`, `Flexible` e `Positioned` usam: o
único filho passa direto.

### 3.2 `ElementCore`

```rust
impl ElementCore {
    pub fn layout_child(&mut self, index: usize, constraints: Constraints, parent_uses_size: bool) -> Size;
    pub fn set_child_offset(&mut self, index: usize, offset: Offset);
    pub fn size(&self) -> Size;
    pub fn offset(&self) -> Offset;
    pub fn mark_needs_layout(&self);
}
```

- `layout_child` é do framework e cuida de cache, fronteiras, flags e validação (seção 4). Quem implementa um element só escreve o algoritmo dele.
- `layout_child` e `set_child_offset` com índice inválido entram em pânico, como indexar um vetor fora do limite.
- `mark_needs_layout` pode ser chamado de qualquer thread, como `mark_dirty`.

### 3.3 `App`

- **`set_size(&mut self, size: Size)`** define o tamanho da raiz: a janela, ou o volume, num app espacial. Se mudou, pede um frame (`Signal::request`). A raiz recebe `Constraints::tight(size)`.
- **`update()`** passa a fazer a passada de build da parte 1 e depois a passada de layout, se houver raiz e tamanho definido. Como `set_root` termina chamando `update`, ele também deixa o layout pronto.
- **`debug_layout(&self) -> String`** devolve uma linha por element, em pré-ordem, com dois espaços de recuo por nível, no formato `{Nome}{ #key} {w}x{h}x{d} @ {x},{y},{z}`. Os números usam o `Display` de `f32`, e a posição é relativa ao pai. Exemplo:

  ```
  Row 300x40x0 @ 0,0,0
    SizedBox 100x40x0 @ 0,0,0
    Expanded 200x40x0 @ 100,0,0
      SizedBox 200x40x0 @ 0,0,0
  ```

### 3.4 `impossible_layout`

```rust
pub fn impossible_layout(message: &str);
```

Em debug (`cfg!(debug_assertions)`), entra em pânico com
`layout impossível: {message}`. Em release, não faz nada, e quem chamou
aplica a própria regra de degradação. Widgets prontos e elements de
terceiros usam a mesma regra.

## 4. Layout incremental

### 4.1 Estado

**`Flags`** ganha três campos atômicos (`SeqCst`), ao lado de
`dirty`/`descendant_dirty`:
- `needs_layout`: começa **ligado** em elements novos;
- `descendant_needs_layout`;
- `relayout_boundary`: gravado a cada layout do element.

**`ElementCore`** ganha `constraints: Option<Constraints>` (as últimas recebidas), `parent_uses_size: bool`, `size: Size` e `offset: Offset`.

### 4.2 Fronteira de relayout

Um element é fronteira quando, no último layout, valia
`!parent_uses_size || element.sized_by_parent() || constraints.is_tight() || é a raiz`.
O tamanho de uma fronteira não pode mudar sem que as restrições dela mudem.

### 4.3 Marcação

**`mark_needs_layout`** (qualquer thread):
1. Liga `needs_layout` no element. Se já estava ligado, para aqui, porque os ancestrais já foram tratados.
2. Se o element não é fronteira e tem pai, repete o passo 1 no pai.
3. Ao chegar numa fronteira (ou na raiz), sobe ligando `descendant_needs_layout` nos ancestrais, parando no primeiro já ligado.
4. Chama `signal.request()`.

**`mark_needs_layout_quiet`** (interno) faz os passos 1 a 3 sem o 4. O
framework usa essa versão durante o `update`.

**Quem marca:**
- todo element **reconstruído** na passada de build se marca (quiet);
- um element reconstruído que tem `parent_data()` marca também o pai (quiet), porque é o pai que lê esses dados;
- elements novos já nascem com `needs_layout`.

### 4.4 Passada de layout

O `update()` chama `layout_element(raiz, Constraints::tight(size), parent_uses_size: false)`.

**`layout_element(e, c, parent_uses_size)`**, que é o que `layout_child` faz:
1. Valida `c` (seção 6).
2. Calcula a fronteira (seção 4.2) e a grava em `relayout_boundary`.
3. Se `needs_layout` está desligado e `c` é igual às restrições guardadas:
   - se `descendant_needs_layout` estiver ligado, chama `relayout_descendants(e)`;
   - devolve o tamanho guardado.
4. Caso contrário:
   1. desliga `needs_layout` e `descendant_needs_layout`, **antes** de chamar o element;
   2. chama `e.layout(c)`;
   3. valida o tamanho devolvido (seção 6);
   4. grava `c`, `parent_uses_size` e o tamanho;
   5. devolve o tamanho.

**`relayout_descendants(e)`:** desliga o `descendant_needs_layout` de `e` e percorre os filhos:
- filho com `needs_layout`: `layout_element(filho, restrições guardadas, parent_uses_size guardado)`. Ele é fronteira, então o tamanho não muda;
- filho só com `descendant_needs_layout`: chama `relayout_descendants(filho)` recursivamente.

### 4.5 Garantias

- Galhos limpos não são visitados.
- Um element limpo que recebe as mesmas restrições não refaz o layout, e as posições que o pai definiu continuam valendo.
- Os widgets prontos fazem o layout de cada filho uma única vez por passada.
- Pai antes do filho.
- Um `mark_needs_layout` concorrente, vindo de outra thread durante o layout, fica para o próximo `update()`, pelo mesmo protocolo de "limpar antes de trabalhar e propagar para cima" da parte 1.
- Um pânico durante o layout envenena o `App`, como na parte 1.

## 5. Widgets (`widgets/`)

**Estilo de API.** Cada widget é uma struct com campos privados. `new(...)`
recebe o obrigatório, métodos encadeados cuidam do opcional, e os filhos são
`Rc<dyn Widget>`. Cada widget tem o próprio element, que implementa `layout`.
Nas fórmulas, `c` são as restrições recebidas.

### 5.1 `ConstrainedBox` e `SizedBox` (`widgets/constrained.rs`)

**`ConstrainedBox::new(extra: Constraints)`**, com `.child(w)`:
- as restrições internas são `extra.enforce(c)`;
- com filho, o filho recebe as restrições internas e o tamanho é o dele;
- sem filho, o tamanho é `interna.constrain(Size::ZERO)`.

**`SizedBox::new()`**, com `.width(v)`, `.height(v)`, `.depth(v)` e `.child(w)`:
- equivale a um `ConstrainedBox` com mínimo e máximo iguais a v nos eixos informados, e de 0 a ∞ nos demais;
- `SizedBox::expand()` usa ∞ nos três eixos;
- `SizedBox::shrink()` usa 0 nos três eixos.

### 5.2 `Padding` (`widgets/padding.rs`)

**`Padding::new(insets, child)`:**
- o filho recebe `c.deflate(insets)` e fica em `Offset::new(left, top, back)`;
- o tamanho é `c.constrain(filho + (horizontal, vertical, depth))`.

### 5.3 `Align` e `Center` (`widgets/align.rs`)

**`Align::new(alignment, child)`**, com `.width_factor(f)`, `.height_factor(f)` e `.depth_factor(f)`:
- o filho recebe `c.loosen()`;
- em cada eixo, o tamanho é:
  - `filho * fator`, se houver fator;
  - o máximo, se o eixo for limitado;
  - senão, o tamanho do filho;
- o tamanho final passa por `c.constrain`;
- a posição do filho é `alignment.offset_of(filho, tamanho)`.

**`Center::new(child)`** é um tipo próprio, para aparecer com esse nome no
debug, com o mesmo layout de `Align::new(Alignment::CENTER, child)`.

### 5.4 `Flex`, `Row`, `Column`, `Expanded`, `Flexible`, `Spacer` (`widgets/flex.rs`)

**`Flex::new(axis, children)`**, `Row::new(children)` (X) e `Column::new(children)` (Y):
- `Row` e `Column` são tipos próprios com o mesmo algoritmo;
- métodos encadeados:
  - `.main_axis_alignment(MainAxisAlignment)`: `Start` (padrão), `End`, `Center`, `SpaceBetween`, `SpaceAround` ou `SpaceEvenly`;
  - `.cross_axis_alignment(CrossAxisAlignment)`: `Start`, `End`, `Center` (padrão) ou `Stretch`. Vale para os dois eixos transversais;
  - `.main_axis_size(MainAxisSize)`: `Max` (padrão) ou `Min`;
  - `.spacing(f32)`.

**Filhos flexíveis:**
- **`Expanded::new(child)`** e **`Flexible::new(child)`** têm `.flex(u32)`, com padrão 1.
- O `parent_data` deles é `FlexParentData { flex: u32, fit: FlexFit }`, um tipo público. `FlexFit` é `Tight` no `Expanded` e `Loose` no `Flexible`.
- **`Spacer::new()`**, com `.flex(u32)`, é um flexível exato, sem filho e de tamanho mínimo.

**Algoritmo** (`main` é o eixo do `Flex`; os outros dois são transversais; `n` é o número de filhos; nas fórmulas, `spacing·(n−1)` vale 0 quando n ≤ 1, e sem filhos não há posições a calcular):
1. **Restrição de um filho em cada eixo transversal t:**
   - `Stretch` com t limitado: exata em `c.max(t)`;
   - `Stretch` com t sem limite: layout impossível;
   - caso contrário: de 0 a `c.max(t)`.
2. **Filhos sem flex** (`flex` 0, ou sem `FlexParentData`): eixo principal de 0 a ∞.
3. **`total_flex` > 0:** exige `c.max(main)` finito; senão, layout impossível.
   - `livre = max(0, c.max(main) − soma dos sem flex − spacing·(n−1))`;
   - cada filho flex recebe `alocado = livre · flex / total_flex`, exato em `Tight` e de 0 a `alocado` em `Loose`.
4. **Tamanho:**
   - `usado = soma dos filhos no eixo principal + spacing·(n−1)`;
   - no eixo principal: `c.max(main)` se `Max` e limitado, senão `usado`;
   - em cada transversal: `c.max(t)` se `Stretch` e limitado, senão o maior filho;
   - tudo passa por `c.constrain`.
5. **Posições no eixo principal**, com `sobra = tamanho − usado`:

   | Alinhamento | Início | Entre filhos |
   |---|---|---|
   | `Start` | 0 | `spacing` |
   | `End` | `sobra` | `spacing` |
   | `Center` | `sobra / 2` | `spacing` |
   | `SpaceBetween` | 0 | `spacing + sobra/(n−1)` (com n = 1: início 0) |
   | `SpaceAround` | `sobra/n/2` | `spacing + sobra/n` |
   | `SpaceEvenly` | `sobra/(n+1)` | `spacing + sobra/(n+1)` |

   Se `sobra` < 0, o alinhamento é ignorado: início 0 e `spacing` entre filhos. Os filhos transbordam a partir do início, sem erro.
6. **Posições transversais:** `Start` e `Stretch` em 0, `End` em `tamanho − filho`, `Center` na metade disso.
7. **No eixo Z,** "início" é o fundo.

### 5.5 `Stack` e `Positioned` (`widgets/stack.rs`)

**`Stack::new(children)`**, com `.alignment(Alignment)` (padrão `TOP_LEFT`) e `.fit(StackFit)`.

**`Positioned::new(child)`:**
- insets `.left`, `.top`, `.right`, `.bottom`, `.back` e `.front`;
- tamanhos `.width`, `.height` e `.depth`;
- o `parent_data` é `PositionedParentData`, um tipo público com esses nove campos `Option<f32>`.

**Filhos não posicionados** (sem `PositionedParentData`) recebem, conforme o `StackFit`:
- `Loose` (padrão): `c.loosen()`;
- `Expand`: exata em `c.biggest()`. Num eixo sem limite isso é layout impossível, e em release esse eixo fica de 0 a ∞;
- `Passthrough`: `c`.

**Tamanho:**
- se houver filhos não posicionados: o maior deles em cada eixo, passado por `c.constrain`;
- senão: `c.biggest()` nos eixos limitados e `c.min` nos demais.

Os não posicionados ficam em `alignment.offset_of(filho, tamanho)`.

**Filhos posicionados.** Os pares de cada eixo são X (`left`/`right`/`width`), Y (`top`/`bottom`/`height`) e Z (`back`/`front`/`depth`).
- **Restrição em cada eixo:**
  - início e fim informados: exata em `max(0, tamanho − início − fim)`;
  - tamanho informado: exata nele;
  - nenhum dos dois: de 0 a ∞.
- **Posição em cada eixo:**
  - início informado: o início;
  - só o fim informado: `tamanho − fim − filho`;
  - nenhum: pelo `alignment`.

### 5.6 `AspectRatio` (`widgets/aspect_ratio.rs`)

**`AspectRatio::new(razão, child)`**, com a razão sendo largura/altura no plano XY. O tamanho no plano segue o algoritmo do Flutter:
1. Se `c` é exata em X e Y, usa o mínimo.
2. Senão:
   - `w = c.max_width`;
   - se `w` for finito, `h = w / razão`;
   - senão: `h = c.max_height`, e com `h` infinito o layout é impossível; caso contrário, `w = h · razão`.
3. Ajustes, nesta ordem:
   - `w > max_w`: w = max_w e h = w / razão;
   - `h > max_h`: h = max_h e w = h · razão;
   - `w < min_w`: w = min_w e h = w / razão;
   - `h < min_h`: h = min_h e w = h · razão.
4. Passa o resultado por `c.constrain`.

O filho recebe tamanho exato `(w, h)` no plano e o intervalo de `c` na
profundidade. A profundidade do `AspectRatio` é a do filho, ou `c.min_depth`
se não houver filho.

### 5.7 `Wrap` (`widgets/wrap.rs`)

**`Wrap::new(children)`**, com métodos encadeados:
- `.direction(Axis)`: X (padrão) ou Y. `Axis::Z` é layout impossível, e em release o `Wrap` usa X;
- `.spacing(f32)` e `.run_spacing(f32)`;
- `.alignment(WrapAlignment)` e `.run_alignment(WrapAlignment)`, com os mesmos seis valores de `MainAxisAlignment`;
- `.cross_axis_alignment(WrapCrossAlignment)`: `Start`, `End` ou `Center`.

**Algoritmo** (`main` é a direção, `cross` é o outro eixo do plano):
1. **Restrições dos filhos:** `main` de 0 a `c.max(main)`, `cross` de 0 a ∞ e profundidade de 0 a `c.max_depth`.
2. **Linhas:** cada filho entra na linha atual se `extensão_atual + spacing + filho ≤ c.max(main)`, considerando o `spacing` só se a linha não estiver vazia. Senão, abre uma nova linha. Com o eixo principal sem limite, tudo fica numa linha.
3. **Tamanho:**
   - `main` = a maior extensão de linha;
   - `cross` = soma das alturas das linhas + `run_spacing·(linhas−1)`, com espaçamento 0 se houver até uma linha;
   - profundidade = o filho mais profundo;
   - tudo passa por `c.constrain`, como no Flutter: o `Wrap` se ajusta ao conteúdo.
4. **Posições:**
   - `alignment` distribui, em cada linha, a sobra de `main`, com as fórmulas do `Flex`;
   - `run_alignment` distribui a sobra de `cross` entre as linhas;
   - `cross_axis_alignment` alinha cada filho dentro da altura da sua linha;
   - z = 0.

### 5.8 `Grid` (`widgets/grid.rs`)

**`Grid::new(colunas: usize, children)`**, com `.row_spacing(f32)`, `.column_spacing(f32)` e `.row_height(GridRowHeight)`:
- `GridRowHeight::AspectRatio(r)` (padrão `AspectRatio(1.0)`) ou `GridRowHeight::Content`;
- exige `c.max_width` finito e pelo menos 1 coluna. Senão, o layout é impossível, e em release o `Grid` assume `c.smallest()` e os filhos recebem `tight(Size::ZERO)`.

**Algoritmo:**
1. `largura_célula = max(0, (c.max_width − column_spacing·(colunas−1)) / colunas)`.
2. **Restrições dos filhos:**
   - `AspectRatio(r)`: exata em `(largura_célula, largura_célula / r)` no plano;
   - `Content`: largura exata, altura de 0 a ∞, e a altura da linha é a do filho mais alto dela. Os filhos ficam no topo da célula.

   Nos dois modos, a profundidade vai de 0 a `c.max_depth`.
3. Os filhos preenchem linha a linha. A posição de cada um é `x = coluna·(largura_célula + column_spacing)`, `y = soma das linhas anteriores + row_spacing·linha` e `z = 0`.
4. O tamanho é `(c.max_width, soma das linhas + row_spacing·(linhas−1), filho mais profundo)`, passado por `c.constrain`. Com até uma linha não há espaçamento, e sem filhos a altura e a profundidade são 0.

### 5.9 Dados de pai fora do lugar

`Expanded`, `Flexible`, `Spacer` e `Positioned` fora de um `Flex` ou
`Stack` não têm efeito: o pai não lê os dados. O layout deles é o padrão, em
que o filho passa direto. Isso fica na documentação desses widgets.

## 6. Validação e erros

- **Restrições inválidas em `layout_element`** (NaN, valor negativo, mínimo maior que o máximo, mínimo infinito):
  - em debug, pânico com `restrições inválidas: {c:?}`;
  - em release, são normalizadas: NaN e negativos viram 0, mínimo infinito vira 0, e um mínimo acima do máximo é rebaixado ao máximo.
- **Tamanho devolvido por `Element::layout`** fora de `c` ou não finito:
  - em debug, pânico com `{Tipo}: tamanho {s:?} fora das restrições {c:?}`, em que `Tipo` é o nome curto do widget;
  - em release, valores não finitos viram o mínimo do eixo, e o resultado passa por `c.constrain`.
- **Layouts impossíveis** (seção 5) usam `impossible_layout`, com estas mensagens:

  | Caso | Mensagem (depois de `layout impossível: `) | Em release |
  |---|---|---|
  | flex com eixo principal sem limite | `Flex com filhos flexíveis precisa de eixo principal limitado` | os filhos flex recebem 0 (exata em 0 ou 0..0) |
  | stretch com eixo transversal sem limite | `Flex com Stretch precisa de eixos transversais limitados` | aquele eixo usa `Start` |
  | `Stack` com `Expand` em eixo sem limite | `Stack com StackFit::Expand precisa de eixos limitados` | aquele eixo fica de 0 a ∞ |
  | `AspectRatio` sem limite em largura e altura | `AspectRatio precisa de largura ou altura limitada` | `c.smallest()` |
  | `Wrap` com `Axis::Z` | `Wrap só aceita Axis::X ou Axis::Y` | usa X |
  | `Grid` sem largura limitada | `Grid precisa de largura limitada` | `c.smallest()` e filhos com `tight(Size::ZERO)` |
  | `Grid` com 0 colunas | `Grid precisa de pelo menos uma coluna` | igual ao anterior |

## 7. Estrutura de arquivos

| Arquivo | Conteúdo |
|---|---|
| `notify-rs/src/geometry.rs` | `Axis`, `Size`, `Offset`, `Constraints`, `EdgeInsets`, `Alignment` |
| `notify-rs/src/layout.rs` | `layout_element`, `relayout_descendants`, validações, `impossible_layout` e o layout padrão |
| `notify-rs/src/flags.rs` | as flags de layout e `mark_needs_layout`/`_quiet` |
| `notify-rs/src/element.rs` | os ganchos novos do `Element` e os campos e métodos novos do `ElementCore` |
| `notify-rs/src/app.rs` | `set_size`, a passada de layout no `update`, `debug_layout` e a marcação na passada de build |
| `notify-rs/src/widgets/mod.rs` | reexporta os widgets |
| `notify-rs/src/widgets/{constrained,padding,align,flex,stack,aspect_ratio,wrap,grid}.rs` | um arquivo por família |
| `notify-rs/src/lib.rs` | reexporta geometria, `impossible_layout`, widgets, enums e tipos de `parent_data` |

## 8. Testes

Os testes ficam ao lado de cada módulo.

- **Geometria:** todas as operações de `Constraints`, nos três eixos, inclusive com infinito; `EdgeInsets` (os totais); `Alignment::offset_of`.
- **Widgets:** tamanhos e posições conferidos pelo `debug_layout`, numa janela plana (`Size::flat`) e, onde a profundidade importa, num volume:
  - **`Flex`:** os seis alinhamentos do eixo principal, os quatro transversais, `Min`/`Max`, `spacing`, `Expanded`/`Flexible`/`Spacer` com fatores diferentes, transbordamento, `Flex` em Z;
  - **`Stack`:** os três `StackFit`, `Positioned` com insets nos três eixos e com tamanho;
  - **`Wrap`:** quebra de linha, `spacing`/`run_spacing`, os alinhamentos;
  - **`Grid`:** os dois modos de altura, espaçamentos e uma última linha incompleta;
  - **os demais widgets:** pelo menos um caso com filho e um sem filho.
- **Layout incremental.** Um element de teste registra cada chamada de `layout` num `Log`. Cenários:
  1. mudar um element dentro de um pai com restrições exatas não refaz o layout dos ancestrais;
  2. `parent_uses_size = false` e `sized_by_parent` também viram fronteira;
  3. um element limpo com as mesmas restrições não refaz o layout;
  4. `set_size` com outro tamanho refaz o layout da raiz, e com o mesmo tamanho não refaz nada;
  5. mudar o `flex` de um `Expanded` refaz o layout do `Flex` pai;
  6. `mark_needs_layout` de outra thread faz `needs_update()` ficar verdadeiro e refaz o layout no próximo `update`;
  7. num `update` sem mudanças, nenhum `layout` é chamado.
- **Validação e erros:**
  - pânicos em debug: restrições inválidas, tamanho fora das restrições num element de teste, e cada caso de layout impossível;
  - comportamento em release: testes com `#[cfg(not(debug_assertions))]`;
  - a verificação final roda `cargo test --workspace` e também `cargo test --workspace --release`.
- **Integração (`tests/`):** um element de layout próprio (`Diagonal`, que posiciona os filhos em diagonal) escrito só com a API pública, como o `opengl-notify-rs` faria.

## 9. Consequências para as próximas partes

- **Parte 3 (pintura):**
  - planos posicionados no espaço;
  - transformações 4x4;
  - recorte de transbordamento e aviso visual;
  - widget de erro;
  - flags `needs_paint` no mesmo padrão de propagação.
- **Parte 6 (entrada):** hit testing por raio no espaço, usando `size()` e `offset()` de cada element.
- **Sem prazo:** tamanho intrínseco (para `Table` e afins), baseline, direita-para-esquerda e `GlobalKey`.
