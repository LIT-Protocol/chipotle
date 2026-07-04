fn main() {
    println!("cargo:rerun-if-changed=./schema");
    println!("cargo:rerun-if-changed=../grpc/schema");

    // Reuse the op-loop message types from lit-actions-grpc instead of
    // generating duplicates: every op type referenced by lit_guest.proto is
    // extern_path'd to its canonical Rust path, so the guest interface and
    // the op-loop share one set of types and cannot drift on the wire.
    let ops = [
        "Print",
        "SetResponse",
        "IncrementFetchCount",
        "AesEncrypt",
        "AesDecrypt",
        "GetPrivateKey",
        "GetLitActionPrivateKey",
        "GetLitActionPublicKey",
        "GetLitActionWalletAddress",
    ];
    let mut builder = tonic_build::configure()
        .protoc_arg("--experimental_allow_proto3_optional")
        .btree_map(["Job.http_headers"]);
    for op in ops {
        builder = builder
            .extern_path(
                format!(".com.litprotocol.actions.ExecuteJsResponse.{op}Request"),
                format!("::lit_actions_grpc::proto::execute_js_response::{op}Request"),
            )
            .extern_path(
                format!(".com.litprotocol.actions.ExecuteJsRequest.{op}Response"),
                format!("::lit_actions_grpc::proto::execute_js_request::{op}Response"),
            );
    }
    builder
        .compile_protos(&["schema/lit_guest.proto"], &["schema/", "../grpc/schema/"])
        .unwrap();
}
