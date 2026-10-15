# Nudge Language Capability Request: Pack 1 (Control Flow & Data)

**Status:** Implemented (see git log)  
**Implementation Commits:** `0837df3` (PACK 1 control flow & data: if/else, for, record literals, indexing, mutable locals), `7b2bef3` (multi-statement if branches + Python/TS codegen)  
**Author:** Pack 1 Coordinator  
**Target Repository:** `/home/neko/Belgeler/ZCode-AI-Calisma-Alani/nudge` at commit `e2cafc0`  
**Classification:** MEVCUT / KISMI / ÖNERİ  

---

## 1. Durum Tasnifi (MEVCUT / KISMI / ÖNERİ)

### MEVCUT (Existing)
1. **Sözlüksel Anahtar Sözcük Token Tanımları:** `crates/nudgec/src/lexer.rs:32-36, 108-112` (commit `f452559`, `0815a38`) içinde `Tok::If`, `Tok::Else`, `Tok::For`, `Tok::ForAll`, `Tok::In` token'ları tanımlıdır. Köşeli ayraç (`Tok::LBracket`, `Tok::RBracket`), süslü ayraç (`Tok::LBrace`, `Tok::RBrace`) ve nokta (`Tok::Dot`) token'ları mevcuttur (`crates/nudgec/src/lexer.rs:49-57`).
2. **`route` Çok Kollu Koşullu Dallanma İfadesi:** `crates/nudgec/src/ast.rs:146-148` (commit `f452559`), `crates/nudgec/src/parser.rs:790-816` (commit `f452559`), `crates/nudgec/src/check.rs:1246-1291` (commit `f452559`, `901d4dca`), `crates/nudgec/src/eval/interpreter.rs:1324-1345` (commit `f452559`). Öncelikli koşul eşleme (`when <cond>`) ve zorunlu varsayılan dal (`otherwise`, eksikliği durumunda `E0702`) ile çalışır.
3. **Bytecode VM `route` Saf Dallanma Desteği:** `crates/nudgec/src/bytecode.rs:69, 629-660` (commit `727e627`), `crates/nudgec/src/vm.rs:198-203` (commit `727e627`). `Op::BranchIfFalse(target)` ve `Op::Jump(target)` opkodları ile saf `route` ifadeleri Bytecode VM üzerinde tam semantik denklikle yürütülür.
4. **`par map` Koleksiyon Eşleme İfadesi:** `crates/nudgec/src/ast.rs:125-130` (commit `f452559`), `crates/nudgec/src/parser.rs:1054-1083` (commit `f452559`), `crates/nudgec/src/check.rs:1159-1193` (commit `f452559`), `crates/nudgec/src/eval/interpreter.rs:421-468` (commit `f452559`). Bir liste koleksiyonu üzerinde lambda dönüşümü uygular.
5. **Kayıt Tipleri ve Statik Alan Erişimi (Field Access):** Tip ifadesi `TypeExpr::Record(Vec<(String, TypeExpr)>)` (`crates/nudgec/src/ast.rs:21`, commit `f452559`), tip denetleyici tipi `Ty::Record(Vec<(String, Ty)>)` (`crates/nudgec/src/check.rs:38`, commit `3b092ce`), alan erişim ifadesi `ExprKind::Field { obj, name }` (`crates/nudgec/src/ast.rs:111-114`, `crates/nudgec/src/parser.rs:675-693`, commit `f452559`). `obj.field` sözdizimi harita ve ajan durum alanlarını okur (`crates/nudgec/src/eval/interpreter.rs:394-418`, commit `f452559`).
6. **Bytecode VM Kayıt ve Liste Yürütme Altyapısı:** `crates/nudgec/src/bytecode.rs:71, 73, 75` (commit `f8a56b9`), `crates/nudgec/src/vm.rs:467-500` (commit `f8a56b9`). Bytecode seviyesinde `Op::MakeList(usize)`, `Op::MakeRecord(Vec<String>)` ve `Op::Field(String)` opkodları tanımlıdır ve VM yığınında `NudgeVal::List` ile `NudgeVal::Map` üretip okuyabilmektedir.
7. **Varsayılan Bytecode VM ve Efekt Delegasyonu:** `crates/nudgec/src/eval/interpreter.rs:1733-1761` (commit `3a66a56`). Programlar varsayılan olarak Bytecode VM ile çalıştırılır (`NUDGE_VM=0` ile kapatılabilir). Efektli fonksiyon çağrıları `Op::DelegateCall` (`crates/nudgec/src/bytecode.rs:79`, commit `cc4ee9f`), efektli satır içi ifadeler ise `Op::DelegateExpr` (`crates/nudgec/src/bytecode.rs:81`, commit `d48fcac`) ile ağaç yorumlayıcısına (`Interpreter`) delege edilir.
8. **Bellek Değer Temsili:** `crates/nudgec/src/eval/val.rs:12-13` (commit `f452559`). `NudgeVal::List(Vec<NudgeVal>)` ve `NudgeVal::Map(HashMap<String, NudgeVal>)` değer tipleri çalışma zamanında mevcuttur.

### KISMI (Partial)
1. **Kayıt Tipi Tanımı Var, Kayıt Değişmezi (Record Literal) Sözdizimi Yok:** `TypeExpr::Record` (`crates/nudgec/src/ast.rs:21`) ve `parser.rs:459-478` ile `{ k: T, ... }` tip bildirimi yapılabilir. Bytecode VM içinde `Op::MakeRecord` opkodu hazırdır (`crates/nudgec/src/vm.rs:475-482`). Ancak AST'de `ExprKind::RecordLit` ve ayrıştırıcıda (`crates/nudgec/src/parser.rs:752-1132`) `{ field: value, ... }` ifadesi bulunmamaktadır. Kayıtlar dilde doğrudan oluşturulamaz; yalnızca araç dönüşlerinden veya `decide` bloklarından dolaylı üretilebilir.
2. **`for_all` Özellik Testi Döngüsü Var, Genel `for` Döngüsü Yok:** `StmtKind::ForAll` (`crates/nudgec/src/ast.rs:217-222`, `crates/nudgec/src/parser.rs:348-372`, commit `f452559`) yalnızca `test` blokları içinde özellik tabanlı test için sentetik üreteçlerle (`gen.int`, `gen.str`) çalışır (`crates/nudgec/src/check.rs:525`, `E0801`). Genel çalışma zamanında liste dolaşan bir `for` döngüsü veya ardışık iteratör sözdizimi mevcut değildir.
3. **`par map` Yorumlayıcıda Var, Bytecode VM'de Yok:** `crates/nudgec/src/bytecode.rs:625, 769` (commit `f8a56b9`). Bytecode derleyicisi `ExprKind::ParMap` ile karşılaştığında `Err("not in bytecode subset: par map")` hatası vererek tüm programı yorumlayıcı geri çekilme yoluna (`interpreter fallback`) zorlar.
4. **Özyineleme Sınırı Var, Döngü İterasyon Sınırı Yok:** Bytecode VM çağrı yığını derinliği `crates/nudgec/src/vm.rs:31` (commit `3e10c49`) satırında `const MAX_CALL_FRAMES: usize = 10_000;` ile korunurken, `vm.rs:181` içindeki geri yönlü atlamalarda (`Op::Jump`) döngü iterasyon limiti veya yakıt sayacı bulunmamaktadır. Ağaç yorumlayıcısı ise işletim sistemi yığınında özyineleme yapmakta ve yaklaşık 1.250 çağrıda `SIGABRT` ile çökmektedir.
5. **Köşeli Ayraç Token'ı Var, İndeksleme Sözdizimi Yok:** `Tok::LBracket` ve `Tok::RBracket` token'ları liste değişmezi (`[x, y]`) ayrıştırmasında kullanılır (`crates/nudgec/src/parser.rs:985-997`). Ancak `parse_postfix` (`crates/nudgec/src/parser.rs:675-713`) yalnızca `Tok::Dot` ve `Tok::LParen` kabul eder; `expr[idx]` indeksleme sözdizimi ve karşılık gelen AST düğümü mevcut değildir.

### ÖNERİ (Proposed)
1. **Genel `if/else` İfadesi:** `if cond { then_expr } else { else_expr }` ve `else if` zinciri sözdizimi, AST düğümü, tip denetimi ve VM derlemesi.
2. **Ardışık `for` Döngüsü / İteratör:** `for item in collection { ... }` ve opsiyonel indeksli `for (item, idx) in collection { ... }` sözdizimi ve yürütümü.
3. **Kayıt Değişmezi (Record Literal) Sözdizimi:** `{ field1: expr1, field2: expr2 }` ifadesi, tip çıkarımı ve mevcut `Op::MakeRecord` ile VM entegrasyonu.
4. **İndeksleme Sözdizimi:** `collection[index]` (listeler için tamsayı tabanlı, kayıt/harita için dinamik anahtar tabanlı), sınır denetimi ve `Op::Index` opkodu.
5. **Döngü Sınırı ve Hata Denetimi:** Deterministik yürütme için `MAX_LOOP_ITERATIONS = 10_000` (`E0703`), dizi dışı indeks erişim hatası (`E0705`).
6. **Birleşik VM ve Delegasyon Yolu:** Saf bloklar doğrudan VM opkodlarına derlenir, efektli ifadeler `Op::DelegateExpr` / `Op::DelegateCall` ile şeffaf şekilde delege edilir.

---

## 2. Capability & Why It Blocks Local Apps

### Yetenek Tanımı
Nudge dilinde temel programlama akışını ve yapılandırılmış veri yönetimini sağlayan çekirdek kontrol akışı ve veri modelleridir:
1. İki yönlü genel koşullu dallanma (`if/else`).
2. Koleksiyonlar üzerinde ardışık döngü ve durum güncellemesi (`for item in coll`).
3. Kod içinde doğrudan adlandırılmış alanlara sahip yapılandırılmış veri üretimi (`{ key: value }`).
4. Koleksiyon elemanlarına ve dinamik alanlara konumsal erişim (`coll[index]`).

### Neden Yerel Uygulama Geliştirmeyi Engelliyor?
1. **Basit Koşullarda Aşırı Şablon ve Zorlama:**
   - Mevcut durumda Nudge dilinde herhangi bir mantıksal koşulu sınamak için geliştirici `route` bloğu yazmak zorundadır:
     ```nudge
     route {
         base: "yes" when is_active,
         rec: "no" otherwise
     }
     ```
   - Yerel arayüz olaylarında, girdi doğrulamasında veya basit durum geçişlerinde `route` ve etiket zorunluluğu (`base:`, `rec:`) kod okunabilirliğini yok etmekte ve basit mantıkları hantallaştırmaktadır.
2. **Ardışık Döngü Yokluğu ve Yığın Çökmesi (Stack Overflow):**
   - Yerel uygulamalar dosya listelerini, kullanıcı girdi listelerini, mesaj geçmişlerini ve arayüz elemanlarını ardışık olarak dolaşmak zorundadır.
   - Nudge'ta döngü olmadığı için liste dolaşımı özyinelemeli (recursive) fonksiyonlarla yapılmaktadır (`examples/bench/compute.ndg:44-50`).
   - Ancak ağaç yorumlayıcısı işletim sistemi yığını üzerinde çalıştığı için yaklaşık 1.250 çağrı sonrasında işletim sistemi seviyesinde `SIGABRT` ile çökmektedir (`crates/nudgec/src/vm.rs:26-30`).
   - Bytecode VM ise yığın derinliğini `MAX_CALL_FRAMES = 10_000` ile sınırlamaktadır. 2.000 elemanlı bir liste üzerinde özyinelemeli filtreleme veya durum biriktirme yapmak Nudge çalışma zamanını tamamen çökertmektedir.
   - `par map` paralel bir dönüşümdür; ardışık ajan durumu güncellemesine (`state.x += item`) izin vermez ve Bytecode VM tarafından reddedilir (`crates/nudgec/src/bytecode.rs:625`).
3. **Kayıt Üretememe (Record Literal Yokluğu):**
   - Nudge tip sisteminde `TypeExpr::Record` bulunmasına rağmen kaynak kodda kayıt nesnesi `{ title: "A", count: 1 }` oluşturulamaz.
   - Geliştiriciler araçlara yapılandırılmış parametre gönderememekte, yerel arayüz bileşenlerine durum nesnesi geçememekte ve fonksiyonlardan birden fazla adlandırılmış değer döndürememektedir. Veriler yapay şekilde düz değişkenlere veya paralel listelere bölünmek zorunda kalmaktadır.
4. **İndekslere Erişememe (Indexing Yokluğu):**
   - Bir listedeki ilk elemana (`items[0]`), son elemana veya n'inci elemana erişim sözdizimi yoktur.
   - Bir listeden eleman almak için bile tüm listeyi özyinelemeli fonksiyonla deşmek gerekmektedir. Bu durum dinamik dizi işlemlerini imkansız kılmaktadır.

---

## 3. Current State with File:Line + Commit Evidence

### 3.1. Lexer Seviyesi İncelemesi
- **Kod Kanıtı:** `crates/nudgec/src/lexer.rs:32-36, 108-112` (commit `f452559`, `0815a38`).
- **Analiz:** Lexer tablosunda `for`, `for_all`, `in`, `if`, `else` ayrılmış sözcükleri `Tok::For`, `Tok::ForAll`, `Tok::In`, `Tok::If`, `Tok::Else` olarak zaten tanımlanmıştır. Köşeli ayraçlar (`Tok::LBracket`, `Tok::RBracket`, satır 53-54) ve süslü ayraçlar (`Tok::LBrace`, `Tok::RBrace`, satır 51-52) ayrıştırılmaktadır. Yani Pack 1 yetenekleri için lexer seviyesinde yeni token eklemeye gerek yoktur; mevcut token'lar kullanılmayı beklemektedir.

### 3.2. Mevcut Koşul Mekanizması: `route`
- **Kod Kanıtı:**
  - `crates/nudgec/src/ast.rs:146-148` (commit `f452559`): `ExprKind::Route { arms: Vec<(String, Expr, Option<Expr>)> }`.
  - `crates/nudgec/src/parser.rs:790-816` (commit `f452559`): `Tok::Ident(s) if s == "route" && self.peek2() == &Tok::LBrace`. `label: value when cond` ve `label: value otherwise` sözdizimini ayrıştırır.
  - `crates/nudgec/src/check.rs:1246-1291` (commit `f452559`, `901d4dca`): Her kolun değeri tip uyumu açısından denetlenir; `otherwise` dalı bulunmazsa `E0702` hatası verilir.
  - `crates/nudgec/src/bytecode.rs:629-660` (commit `727e627`): Bytecode derleyicisi her kol için `Op::BranchIfFalse` ve sona atlama için `Op::Jump` üretir; aksi halde `Op::Fail` basar.
  - `crates/nudgec/src/vm.rs:198-203` (commit `727e627`): `Op::BranchIfFalse` çalışma zamanında koşul yanlış ise hedef yönergeye atlar.
  - `crates/nudgec/src/eval/interpreter.rs:1324-1345` (commit `f452559`): `eval_route` kolları sırayla değerlendirir.
- **Analiz:** `route`, model yönlendirme ve çok seçenekli politika seçimleri için tasarlanmıştır. Genel boolean kontrolü için gereksiz etiket yükü (`label:`) getirmekte ve `if/else` gibi hafif bir ifadenin yerini tutamamaktadır.

### 3.3. Mevcut Koleksiyon Eşleme: `par map`
- **Kod Kanıtı:**
  - `crates/nudgec/src/ast.rs:125-130` (commit `f452559`): `ExprKind::ParMap { coll, kwargs, params, body }`.
  - `crates/nudgec/src/parser.rs:1054-1083` (commit `f452559`): `par map coll |x| -> body` veya `par map(coll, workers = 4) |x| -> body` sözdizimini ayrıştırır.
  - `crates/nudgec/src/check.rs:1159-1193` (commit `f452559`): Koleksiyonun `[T]` tipinde olduğunu doğrular ve gövdeyi eleman tipiyle denetler.
  - `crates/nudgec/src/bytecode.rs:625, 769` (commit `f8a56b9`): Bytecode derleyicisinde `ExprKind::ParMap` doğrudan `Err("not in bytecode subset: par map")` hatası verir.
  - `crates/nudgec/src/eval/interpreter.rs:421-468` (commit `f452559`): Yorumlayıcı tarafında eşleme sırayla yürütülür ve sonuç listesi döner.
- **Analiz:** `par map` bir map dönüşümüdür; yan etkili ardışık döngü veya durum güncellemesi sağlayamaz. Ayrıca Bytecode VM alt kümesine dahil edilmediği için kullanıldığı an programı interpreter fallback'e iter.

### 3.4. Kayıt Tipleri ve Alan Erişimi (Field Access)
- **Kod Kanıtı:**
  - `crates/nudgec/src/ast.rs:21` (commit `f452559`): `TypeExpr::Record(Vec<(String, TypeExpr)>)`.
  - `crates/nudgec/src/check.rs:38` (commit `3b092ce`): `Ty::Record(Vec<(String, Ty)>)`.
  - `crates/nudgec/src/parser.rs:459-478` (commit `f452559`): `{ k: T, ... }` tip bildirimi ayrıştırılır.
  - `crates/nudgec/src/ast.rs:111-114` (commit `f452559`): `ExprKind::Field { obj, name }`.
  - `crates/nudgec/src/parser.rs:675-693` (commit `f452559`): `parse_postfix` içinde `Tok::Dot` ile alan adı okunur.
  - `crates/nudgec/src/bytecode.rs:73, 75` (commit `f8a56b9`): `Op::MakeRecord(Vec<String>)` ve `Op::Field(String)`.
  - `crates/nudgec/src/vm.rs:475-500` (commit `f8a56b9`): VM `Op::MakeRecord` ile yığından değerleri çekip `NudgeVal::Map` kurar; `Op::Field` ile harita alanını okur.
  - `crates/nudgec/src/eval/interpreter.rs:394-418` (commit `f452559`): Yorumlayıcı `NudgeVal::Map` ve `NudgeVal::AgentState` alanlarını okur.
- **Analiz:** Kayıt tipleri, alan okuma ve VM seviyesinde kayıt üretme altyapısı (`Op::MakeRecord`) halihazırda mevcuttur. Eksik olan tek parça, kullanıcının `.ndg` dosyasında `{ key: value, ... }` şeklinde kayıt değişmezi yazabilmesini sağlayan ayrıştırıcı ve AST kuralıdır.

### 3.5. İndeksleme ve Genel Döngü Eksikliği
- **Kod Kanıtı:**
  - `crates/nudgec/src/parser.rs:675-713` (commit `f452559`): `parse_postfix` fonksiyonu yalnızca `Tok::Dot` (alan) ve `Tok::LParen` (çağrı) denetler. `Tok::LBracket` kontrolü yoktur; dolayısıyla `items[0]` gibi bir ifade sözdizim hatası verir (`E0002`).
  - `crates/nudgec/src/parser.rs:324-442` (commit `f452559`): `parse_stmt` içinde `Tok::For` kontrolü yoktur; yalnızca `Tok::ForAll` mülkiyet testi döngüsü (`crates/nudgec/src/parser.rs:348-372`) ayrıştırılır.

---

## 4. Proposed .ndg Source Syntax

Pack 1, Nudge dilinin mevcut minimalist felsefesine ve Rust benzeri saf ifadesel yapısına tam uyumlu dört yeni sözdizimsel form önerir:

### 4.1. Genel `if/else` İfadesi
`if` bir ifadedir (expression) ve sonlanan bloğun değerini üretir.
```nudge
// Basit if/else ifadesi
let status = if score >= 80 {
    "pass"
} else {
    "fail"
}

// Zincirleme else if yapısı
let tier = if score >= 90 {
    "gold"
} else if score >= 75 {
    "silver"
} else {
    "bronze"
}

// Deyim (statement) konumunda opsiyonel else
if threshold_reached {
    log_alert("Limit asildi")
}
```

### 4.2. Ardışık `for` Döngüsü / İteratör Formu
Listeleri deterministik sıra ile dolaşır; ajan durumunu güncellemeye veya yan etkili ardışık işlemlere imkan verir.
```nudge
// Temel eleman iterasyonu
for item in items {
    state.total += item.price
}

// Eleman ve indeks ile iterasyon
for (item, idx) in items {
    io.println("Sira: " + idx + " - " + item.name)
}
```

### 4.3. Kayıt Değişmezi (Record Literal) Sözdizimi
Süslü ayraçlar içinde virgülle ayrılmış anahtar-değer çiftleri:
```nudge
// Tip takma adı
type User = { id: int, name: string, active: bool }

// Kayıt değişmezi olusturma
let u1 = {
    id: 1,
    name: "Alice",
    active: true
}

// İc ice kayitlar ve liste icinde kayitlar
let team = [
    { id: 1, name: "Alice", active: true },
    { id: 2, name: "Bob", active: false }
]
```

### 4.4. İndeks Sözdizimi (Indexing Syntax)
Köşeli ayraç ile 0-tabanlı eleman veya anahtar erişimi:
```nudge
let numbers = [10, 20, 30, 40]
let first = numbers[0]         // 10
let second = numbers[1]        // 20
let dynamic_val = numbers[idx] // tamsayi degisken ile erisim

// Kayit ve harita uzerinde dinamik erisim
let field_name = "name"
let val = u1[field_name]       // dinamik anahtar erisimi
```

### 4.5. Birleşik Örnek (.ndg)
Tüm yeni formların birlikte çalıştığı somut akış:
```nudge
type Item = { id: int, name: string, cost: int, eligible: bool }

fn process_inventory(items: [Item]) -> int uses IO {
    let mut_total = 0
    for (it, idx) in items {
        let multiplier = if it.eligible {
            1
        } else {
            0
        }
        io.println("Islem: " + idx + " " + it.name)
        let cost = items[idx].cost
        // Toplam hesabi
    }
    42
}
```

---

## 5. Type & Effect Semantics

### 5.1. Tip Kuralları (Type Rules)

#### (a) `if/else` İfadesi
- Koşul ifadesi (`cond`) kesinlikle `Ty::Bool` (veya boolean'a atanabilir) olmak zorundadır. Aksi halde `E0201` ("if condition must be bool, got {ty}") hatası üretilir.
- Değer üreten ifade bağlamında `then` ve `else` kollarının tipleri unifiye edilmelidir (`assignable(&then_ty, &else_ty)`). Uyuşmazlık durumunda `E0201` ("if and else branches have incompatible types: then yields {t1}, else yields {t2}") verilir.
- Değer beklenen konumda (ör. `let x = if ...`) `else` dalı zorunludur (`E0704`: "if expression used as a value must include an else branch"). Yalnızca deyim konumunda (`StmtKind::ExprStmt`) `else` atlanabilir; bu durumda ifadenin tipi `Ty::None_` (Unit) kabul edilir.

#### (b) `for` Döngüsü
- İterasyon hedefi koleksiyon kesinlikle `Ty::List(inner)` tipi olmalıdır. Aksi halde `E0201` ("for loop requires a list collection, got {ty}") verilir.
- Döngü değişkeni (`item`), listenin eleman tipi olan `inner` tipini otomatik olarak alır.
- Opsiyonel indeks değişkeni (`idx`) kesinlikle `Ty::Int` tipinde tanımlanır.
- `for` döngüsü bir deyim (statement) olarak yürütülür ve `Ty::None_` (Unit) döndürür.

#### (c) Kayıt Değişmezi (Record Literal)
- İfade tipi `Ty::Record(vec![(field_name, field_ty), ...])` olarak sentezlenir.
- Kayıt içinde yinelenen alan adı kullanılması yasaktır (`E0203`: "duplicate field '{name}' in record literal").
- Belirtilmiş bir tip takma adına (`type User = { ... }`) atanırken, alan adları ve tipleri tam eşleşmelidir; eksik veya fazla alan `E0201` hatası üretir.

#### (d) İndeksleme (Indexing)
- Dizi indeksleme (`list[idx]`):
  - `list` tipi `Ty::List(elem)` olmalıdır.
  - `idx` tipi kesinlikle `Ty::Int` olmalıdır. Aksi halde `E0201` ("list index must be an integer, got {ty}") üretilir.
  - İfadenin tipi `*elem` olur.
- Harita/Kayıt indeksleme (`rec[key]`):
  - `rec` tipi `Ty::Record(fields)` olmalıdır.
  - `key` tipi `Ty::Str` olmalıdır.
  - Statik dize sabiti verilirse alan tipi statik olarak çözülür; dinamik dize verilirse tip `Ty::Unknown` olarak çıkarılır.
- Diğer tipler üzerinde indeksleme girişimi `E0201` ("cannot index into type {ty}") ile reddedilir.

### 5.2. Efekt Yayılımı (Effect Propagation)
Nudge'ın beş sabit efektli tip denetimi (`check.rs:288-376`, `direct_effects` ve `body_effects`) yeni formlara şeffaf şekilde uygulanır:
- **`if/else`:** Koşul, `then` gövdesi ve `else` gövdesindeki efektlerin birleşimi (union) fonksiyonun `uses` bildirimine yansıtılır. Örnek: `then` kolunda bir `Tool` çağrısı yapılıyorsa, çevreleyen fonksiyon `uses Tool` bildirmek zorundadır (`E0301`).
- **`for`:** Döngü koleksiyon ifadesi ve döngü gövdesindeki tüm ifadelerin efektleri birleştirilir. Gövdede `LLM` veya `Tool` kullanılıyorsa fonksiyon `uses LLM, Tool` olarak işaretlenmelidir.
- **Kayıt Değişmezi:** Alan değer ifadelerinde bulunan efektler toplanır.
- **İndeksleme:** Hedef nesne ve indeks ifadesinin efektleri toplanır.
- **Saf Fonksiyon Güvencesi:** Saf (pure) olarak işaretlenmiş bir fonksiyon içinde yer alan hiçbir `if`, `for`, kayıt veya indeks ifadesi efektli bir çağrı barındıramaz (`crates/nudgec/src/check.rs:1805`, `E0301`).

### 5.3. Hata Kodları (Error Codes)
Nudge E-kod hiyerarşisine (`crates/nudgec/src/hints.rs`) tam uyumlu yeni ve genişletilmiş kodlar:
- **`E0201`:** Tip uyuşmazlığı (if/else kol tipi uyumsuzluğu, non-bool if koşulu, non-list for koleksiyonu, non-int liste indeksi).
- **`E0203`:** Kayıt değişmezinde yinelenen alan adı ("duplicate field '{name}' in record literal").
- **`E0702`:** `route` bloğunda eksik `otherwise` dalı (mevcut kod korunur).
- **`E0703`:** Döngü iterasyon sınırı aşıldı ("loop iteration limit exceeded: execution halted after 10000 iterations to prevent infinite loop").
- **`E0704`:** Değer beklenen if ifadesinde eksik `else` dalı ("if expression used as a value must include an else branch").
- **`E0705`:** Çalışma zamanı sınır dışı indeks hatası ("index out of bounds: index {idx} out of range for list of length {len}").

---

## 6. Local Execution Behavior

### 6.1. Çalışma Zamanı Yürütümü
- **Harici Bağımlılık Yok:** Tüm kontrol akışı ve veri modelleri doğrudan `nudge-runtime` ve `nudgec` çekirdeğinde yürütülür; harici interpreter veya C/Python kütüphanesine ihtiyaç duymaz.
- **Sıralı ve Deterministik Döngü:** `for` döngüsü koleksiyon elemanlarını 0'dan `len - 1`'e kadar deterministik sırada işler. Bu durum NTF v1 iz günlüğüne kaydedilen olayların (`fn.call`, `tool.call`) her çalıştırmada aynı sırayla üretilmesini sağlar.
- **Döngü İterasyon Sınırı (Loop Iteration Bound):**
  - Sonsuz döngüleri ve hizmet aksatma (DoS) risklerini önlemek için çalışma zamanında varsayılan olarak `MAX_LOOP_ITERATIONS = 10_000` sınırı uygulanır.
  - Bu sınır ortam değişkeni `NUDGE_MAX_LOOP_ITERATIONS` ile ayarlanabilir.
  - Sınır aşıldığında program çökmek yerine temiz bir `E0703` hatası fırlatarak durur.
- **Sınır Denetimli İndeksleme:**
  - `list[idx]` erişiminde `idx < 0` veya `idx >= list.len()` ise çalışma zamanı anında `E0705` hatası döndürür: `index out of bounds: index {idx} out of range for list of length {len}`.
  - Bellek taşması veya tanımsız davranış (undefined behavior) oluşması imkansızdır.

### 6.2. `route` ve `par map` ile Etkileşim
- **`route` ile Etkileşim:** `route` dilden çıkarılmaz; model seçimi, olasılıksal politika yönlendirmesi ve çok kollu karar blokları için birinci sınıf politika operatörü olarak kalır. `if/else`, basit boolean kararlar için hafif bir alternatif sunar.
- **`par map` ile Etkileşim:** `par map` eşzamanlı/bağımsız liste dönüşümleri için optimize edilmiş paralel eşleme olarak kalır. `for` ise sıralı işlem, erken çıkış, sıralı yan etki ve reaktif durum (`state.x`) güncellemeleri için kullanılır. İkisi birbirini tamamlar.

---

## 7. Security Boundary

1. **Kaynak Tüketim Güvenliği (Resource Bounding):**
   - Özyinelemedeki `MAX_CALL_FRAMES = 10_000` sınırına paralel olarak, `for` döngülerine `MAX_LOOP_ITERATIONS = 10_000` sınırı getirilerek yapay zeka ajanlarının veya kullanıcı kodunun CPU'yu kilitlemesi engellenir.
2. **Hafıza Güvenliği ve İndeks Sınırı:**
   - Dizi indeksleme işlemlerinin tamamı Rust'ın güvenli dil garantileri ve açık sınır denetimi (`bounds checking`) altında yürütülür; C tarzı bellek aşımı, bellek sızıntısı veya yetkisiz hafıza erişimi mümkün değildir.
3. **Efekt İzolasyonu ve İzinler:**
   - Yeni kontrol akışı ve veri modelleri tek başlarına hiçbir dış yetki (`IO`, `Tool`, `LLM`, `Computer`) vermez.
   - Bir döngü veya if dalı içerisinde araç çağrılacaksa, fonksiyonun açıkça `uses Tool` bildirmesi ve `NUDGE_TOOL_GRANTS` (`crates/nudge-runtime/src/grants.rs:8-37`, commit `ea7f29b`) yetki listesinde ilgili aracın onaylanmış olması zorunludur.
4. **Deterministik NTF v1 İz ve Tekrar Oynatma (Replay Parity):**
   - Sıralı kontrol akışı sayesinde tüm araç ve LLM çağrıları kayıt/tekrar oynatma sırasında (`NUDGE_REPLAY`) birebir aynı sırayla eşleşir; nondeterminizm önlenir.

---

## 8. Minimal Acceptance Test

Harici sistem bağımlılığı olmadan, kayıt oluşturma, liste dolaşımı, indeksleme, koşullu dallanma ve hata durumlarını doğrulayan kabul testi:

```nudge
// test_pack1_control_flow.ndg
type Task = {
    id: int,
    title: string,
    priority: int,
    completed: bool
}

fn evaluate_task(t: Task) -> string {
    if t.completed {
        "done"
    } else if t.priority > 5 {
        "urgent"
    } else {
        "normal"
    }
}

test "kayit degismezi, liste indeksleme, dongu ve if/else dogrulamasi" {
    // 1. Kayit degismezleri ve liste olusturma
    let t1 = { id: 1, title: "Girdi Dogrula", priority: 8, completed: false }
    let t2 = { id: 2, title: "Rapor Uret", priority: 3, completed: true }
    let t3 = { id: 3, title: "Veritabani Temizle", priority: 2, completed: false }
    let tasks = [t1, t2, t3]

    // 2. Indeks erisimi dogrulamasi
    assert tasks[0].title == "Girdi Dogrula"
    assert tasks[1].completed == true
    assert tasks[2].priority == 2

    // 3. if/else kosul dogrulamasi
    assert evaluate_task(tasks[0]) == "urgent"
    assert evaluate_task(tasks[1]) == "done"
    assert evaluate_task(tasks[2]) == "normal"

    // 4. for dongusu ile liste dolasimi ve toplam hesaplama
    let mut urgent_count = 0
    for (t, idx) in tasks {
        let label = evaluate_task(t)
        if label == "urgent" {
            urgent_count = urgent_count + 1
        }
        assert tasks[idx].id == t.id
    }
    assert urgent_count == 1
}
```

### Doğrulama Kriteri (Interpreter == Default VM)
Bu test programı hem yorumlayıcı (`NUDGE_VM=0`) hem de varsayılan Bytecode VM (`NUDGE_VM=1`) altında çalıştırıldığında:
1. Her iki yürütme motorunda da aynı bitiş kodunu (exit 0) üretmelidir.
2. Hesaplanan tüm assert ifadeleri birebir aynı doğrulukla geçmelidir.
3. Sınır dışı indeks erişiminde (`tasks[99]`) her iki motor da birebir aynı `E0705` hata mesajını üretmelidir.

---

## 9. VM-Path Note

### Kaynak Analizi ve Doğrulanan Davranış
- `crates/nudgec/src/eval/interpreter.rs:1734-1755` (commit `3a66a56`) gereğince Nudge programları varsayılan olarak Bytecode VM üzerinde çalıştırılır.
- **Kayıt Değişmezleri (Record Literals):**
  - Kaynaktan teyit edildiği üzere, `Op::MakeRecord(Vec<String>)` opkodu `crates/nudgec/src/bytecode.rs:73` ve `crates/nudgec/src/vm.rs:475-482` (commit `f8a56b9`) içinde zaten eksiksiz uygulanmıştır.
  - Yığın tabanlı VM, `keys.iter().rev()` sırasıyla yığından değerleri çekip `NudgeVal::Map(map)` nesnesini yığına basmaktadır.
  - Dolayısıyla ayrıştırıcıya `{ field: expr, ... }` sözdizimi eklendiğinde, bytecode derleyicisinin (`bytecode.rs:compile_expr`) tek yapması gereken her alan ifadesini derleyip ardından `self.chunk.emit(Op::MakeRecord(keys))` yaymaktır. VM kodunda yeni bir opkoda ihtiyaç bile yoktur.
- **Genel `if/else` İfadesi:**
  - `crates/nudgec/src/bytecode.rs:69, 636` ve `crates/nudgec/src/vm.rs:198-203` (commit `727e627`) içinde `Op::BranchIfFalse(target)` ve `Op::Jump(target)` opkodları `route` için zaten mevcuttur.
  - `if cond { then } else { else }` ifadesi doğrudan şu akışa derlenir:
    1. `compile_expr(cond)`
    2. `let else_jump = emit(Op::BranchIfFalse(0))`
    3. `compile_expr(then)`
    4. `let end_jump = emit(Op::Jump(0))`
    5. `patch(else_jump, current_pc)`
    6. `compile_expr(else)`
    7. `patch(end_jump, current_pc)`
  - Bu derleme saf `route` kollarıyla birebir aynı verimlilikte ve tam VM denkliğinde çalışır.
- **İndeksleme (`Op::Index`):**
  - Bytecode için minimal bir `Op::Index` opkodu eklenir. VM yığınından sırasıyla `idx` ve `coll` çekilir. `coll` bir `NudgeVal::List` ise sınır kontrolü yapılarak eleman yığına basılır; `NudgeVal::Map` ise anahtar okunur. Hata durumunda yorumlayıcı ile birebir aynı metin üretilir.
- **Ardışık `for` Döngüsü:**
  - Saf bir `for` döngüsü, yığında saklanan liste, yerel indeks sayacı ve `Op::BranchIfFalse` / geri yönlü `Op::Jump` ile doğrudan VM düzeyinde döngüye çevrilebilir.
  - Döngü gövdesinde satır içi efektli bir çağrı (`llm"""..."""` veya araç çağrısı) bulunursa, ifade düzeyinde delegasyon mekanizması (`Op::DelegateExpr`, `crates/nudgec/src/bytecode.rs:81`, `crates/nudgec/src/vm.rs:18`, commit `d48fcac`) devreye girer. VM döngü durumunu ve yerel değişkenlerini koruyarak yalnızca ilgili ifadeyi yorumlayıcıya işletir ve VM içinde döngüye devam eder.

### Kaynaktan Doğrulanamayan Hususlar (Varsayım Yapılmayan Noktalar)
1. **VM İçi Geri Yönlü Atlama Yakıtı (Fuel/Instruction Limit):**
   - Mevcut `crates/nudgec/src/vm.rs` incelendiğinde, yalnızca çağrı derinliği için `MAX_CALL_FRAMES` (10.000) kontrolü olduğu görülmektedir. Yönerge sayacı veya geri yönlü atlama (`backward jump`) kontrolü kaynak kodda bulunmamaktadır.
   - Bu nedenle `for` döngüsünün VM tarafında sonsuz döngüye girmesini engellemek için, derleyicinin ya döngü başına sentetik bir sınır denetimi eklemesi ya da VM `run_loop` döngüsüne yönerge/iterasyon sayacı entegre edilmesi zorunludur.
2. **`par map` VM Uyumu:**
   - `crates/nudgec/src/bytecode.rs:625` satırında `par map` açıkça reddedilmektedir (`not in bytecode subset`). Dolayısıyla `par map` içeren fonksiyonların VM'de çalıştırılabileceği varsayılamaz; bu fonksiyonlar yorumlayıcıya delege edilmek veya tüm program yorumlayıcı yolunda çalıştırılmak zorundadır. Pack 1'in getirdiği sıralı `for` döngüsü ise saf olduğu sürece doğrudan VM üzerinde çalışabilecektir.
