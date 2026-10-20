//! release の埋め込みを web/ の変更に追従させる。
//!
//! rust-embed は release で各ファイルを include_bytes! で埋め込むため、Cargo が追うのは
//! その時点で在ったファイルだけで、web/ にファイルを足しても再コンパイルされない。
//! ディレクトリを指すと Cargo は中を走査して変更を見る。

fn main() {
    println!("cargo::rerun-if-changed=../../web");
}
