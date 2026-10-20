// Copyright © 2026 1o. All rights reserved.

//! The probe crate: one call of each toolkit piece on placeholders, written by the change vocabulary's renderer;
//! and the catalog read from klepto's expansion of it.

use super::normal::{self, Level};
use super::{Catalog, Hole, HoleKind, Position, ToolkitShape, SHAPES_SCHEMA};
use crate::change::{self, FnSpec, Param, Piece, TOOLKIT};
use quote::ToTokens;
use std::collections::BTreeSet;
use std::path::Path;
use syn::visit::Visit;
use syn::visit_mut::VisitMut;

/// A method piece's function: its receiver, parameters (name, type; an empty type is a bare name), return type,
/// and whether it is `const`.
#[derive(Debug, Clone, Copy)]
struct Sig {
    receiver: &'static str,
    params: &'static [(&'static str, &'static str)],
    ret: Option<&'static str>,
    is_const: bool,
}

const fn sig(receiver: &'static str, params: &'static [(&'static str, &'static str)], ret: Option<&'static str>) -> Option<Sig> {
    Some(Sig { receiver, params, ret, is_const: false })
}

const fn const_sig(receiver: &'static str, params: &'static [(&'static str, &'static str)], ret: Option<&'static str>) -> Option<Sig> {
    Some(Sig { receiver, params, ret, is_const: true })
}

/// One call of a piece: which call, where it stands, its slots (a list slot holds several placeholders), and a
/// method piece's function.
#[derive(Debug, Clone, Copy)]
struct Probe {
    piece: &'static str,
    variant: &'static str,
    position: Position,
    slots: &'static [(&'static str, &'static [&'static str])],
    sig: Option<Sig>,
}

const fn p(piece: &'static str, variant: &'static str, position: Position, slots: &'static [(&'static str, &'static [&'static str])], sig: Option<Sig>) -> Probe {
    Probe { piece, variant, position, slots, sig }
}

use Position::{Function, ImplFn, Step, Value};

const C2: &[&str] = &["__si_conditions_0", "__si_conditions_1"];
const C3: &[&str] = &["__si_conditions_0", "__si_conditions_1", "__si_conditions_2"];
const C4: &[&str] = &["__si_conditions_0", "__si_conditions_1", "__si_conditions_2", "__si_conditions_3"];
const E1: &[&str] = &["__si_errors_0"];
const E2: &[&str] = &["__si_errors_0", "__si_errors_1"];
const C1: &[&str] = &["__si_conditions_0"];
const SELF_FIELD: &[(&str, &[&str])] = &[("field", &["__sid_field"])];
const SELF_PATH: &[(&str, &[&str])] = &[("field", &["__sif_field"])];

/// Every call the catalog is made of. A piece's slots and its function follow its macro's matcher in
/// simple_impl 0.1.12 (`src/macros/toolkit.rs`, `src/macros/mod.rs`): an `$x:ident` is a `__sid_` placeholder, a
/// `$x:ty` a `__Si` one, `$first:ident $(.$rest:ident)*` a `__sif_` one.
const PROBES: &[Probe] = &[
    // pure checks: values
    p("count_true", "2 conditions", Value, &[("conditions", C2)], None),
    p("count_true", "3 conditions", Value, &[("conditions", C3)], None),
    p("count_true", "4 conditions", Value, &[("conditions", C4)], None),
    p("all", "2 conditions", Value, &[("conditions", C2)], None),
    p("all", "3 conditions", Value, &[("conditions", C3)], None),
    p("all", "4 conditions", Value, &[("conditions", C4)], None),
    p("any", "2 conditions", Value, &[("conditions", C2)], None),
    p("any", "3 conditions", Value, &[("conditions", C3)], None),
    p("any", "4 conditions", Value, &[("conditions", C4)], None),
    p("exactly_one", "2 conditions", Value, &[("conditions", C2)], None),
    p("exactly_one", "3 conditions", Value, &[("conditions", C3)], None),
    p("at_most_one", "2 conditions", Value, &[("conditions", C2)], None),
    p("at_most_one", "3 conditions", Value, &[("conditions", C3)], None),
    p("implies", "", Value, &[("left", &["__si_left"]), ("right", &["__si_right"])], None),
    p("excludes", "", Value, &[("left", &["__si_left"]), ("right", &["__si_right"])], None),
    p("in_range", "", Value, &[("value", &["__si_value"]), ("minimum", &["__si_minimum"]), ("maximum", &["__si_maximum"])], None),
    p("nonempty", "", Value, &[("value", &["__si_value"])], None),
    p("is_one_of", "2 patterns", Value, &[("value", &["__si_value"]), ("candidates", &["__sip_candidates_0", "__sip_candidates_1"])], None),
    p("is_one_of", "3 patterns", Value, &[("value", &["__si_value"]), ("candidates", &["__sip_candidates_0", "__sip_candidates_1", "__sip_candidates_2"])], None),
    // guards
    p("guard", "", Step, &[("condition", &["__si_condition"]), ("return_value", &["__si_return_value"])], None),
    p("ensure", "", Step, &[("condition", &["__si_condition"]), ("error", &["__si_error"])], None),
    p("bail", "", Step, &[("error", &["__si_error"])], None),
    p("ensure_eq", "", Step, &[("left", &["__si_left"]), ("right", &["__si_right"]), ("error", &["__si_error"])], None),
    p("ensure_ne", "", Step, &[("left", &["__si_left"]), ("right", &["__si_right"]), ("error", &["__si_error"])], None),
    p("ensure_nonempty", "", Step, &[("value", &["__si_value"]), ("error", &["__si_error"])], None),
    p("ensure_empty", "", Step, &[("value", &["__si_value"]), ("error", &["__si_error"])], None),
    p("ensure_len", "", Step, &[("value", &["__si_value"]), ("expected", &["__si_expected"]), ("error", &["__si_error"])], None),
    p("ensure_min_len", "", Step, &[("value", &["__si_value"]), ("minimum", &["__si_minimum"]), ("error", &["__si_error"])], None),
    p("ensure_max_len", "", Step, &[("value", &["__si_value"]), ("maximum", &["__si_maximum"]), ("error", &["__si_error"])], None),
    p("ensure_range", "", Step, &[("value", &["__si_value"]), ("minimum", &["__si_minimum"]), ("maximum", &["__si_maximum"]), ("error", &["__si_error"])], None),
    p("ensure_any", "2 conditions", Step, &[("error", &["__si_error"]), ("conditions", C2)], None),
    p("ensure_any", "3 conditions", Step, &[("error", &["__si_error"]), ("conditions", C3)], None),
    p("ensure_exactly_one", "2 conditions", Step, &[("error", &["__si_error"]), ("conditions", C2)], None),
    p("ensure_at_most_one", "2 conditions", Step, &[("error", &["__si_error"]), ("conditions", C2)], None),
    p("ensure_implies", "", Step, &[("left", &["__si_left"]), ("right", &["__si_right"]), ("error", &["__si_error"])], None),
    p("ensure_excludes", "", Step, &[("left", &["__si_left"]), ("right", &["__si_right"]), ("error", &["__si_error"])], None),
    p("ensure_valid", "", Step, &[("value", &["__si_value"])], None),
    p("ensure_valid", "with map_error", Step, &[("value", &["__si_value"]), ("map_error", &["__si_map_error"])], None),
    // extraction and early flow
    p("require_some", "", Value, &[("option", &["__si_option"]), ("error", &["__si_error"])], None),
    p("require_ok", "", Value, &[("result", &["__si_result"]), ("map_error", &["__si_map_error"])], None),
    p("some_or_return", "", Value, &[("option", &["__si_option"]), ("return_value", &["__si_return_value"])], None),
    p("ok_or_return", "", Value, &[("result", &["__si_result"]), ("return_value", &["__si_return_value"])], None),
    p("return_if", "", Step, &[("condition", &["__si_condition"]), ("return_value", &["__si_return_value"])], None),
    p("return_unless", "", Step, &[("condition", &["__si_condition"]), ("return_value", &["__si_return_value"])], None),
    p("none_if", "", Step, &[("condition", &["__si_condition"])], None),
    p("break_if", "", Step, &[("condition", &["__si_condition"])], None),
    p("break_if", "with a value", Step, &[("condition", &["__si_condition"]), ("value", &["__si_value"])], None),
    p("continue_if", "", Step, &[("condition", &["__si_condition"])], None),
    p("some_or_continue", "", Value, &[("option", &["__si_option"])], None),
    p("some_or_break", "", Value, &[("option", &["__si_option"])], None),
    p("require_ok_into", "", Value, &[("result", &["__si_result"]), ("map_error", &["__si_map_error"])], None),
    p("require_ok_into", "a statement", Step, &[("result", &["__si_result"]), ("map_error", &["__si_map_error"])], None),
    p("require_some_into", "ok_or", Value, &[("option", &["__si_option"]), ("error", &["__si_error"])], None),
    p("require_some_into", "ok_or_else", Value, &[("option", &["__si_option"]), ("error", &["|| __si_error"])], None),
    p("ok_or_else_return", "", Value, &[("result", &["__si_result"]), ("item", &["__sid_item"]), ("return_value", &["__si_return_value"])], None),
    p("return_if_err", "", Step, &[("result", &["__si_result"]), ("item", &["__sid_item"]), ("return_value", &["__si_return_value"])], None),
    // values chosen
    p("if_else", "", Value, &[("condition", &["__si_condition"]), ("left", &["__si_left"]), ("right", &["__si_right"])], None),
    p("table", "3 entries", Value, &[("value", &["__si_value"]), ("candidates", &["__sip_candidates_0", "__sip_candidates_1", "__sip_candidates_2"]), ("args", &["__si_args_0", "__si_args_1", "__si_args_2"])], None),
    p("table", "4 entries", Value, &[("value", &["__si_value"]), ("candidates", &["__sip_candidates_0", "__sip_candidates_1", "__sip_candidates_2", "__sip_candidates_3"]), ("args", &["__si_args_0", "__si_args_1", "__si_args_2", "__si_args_3"])], None),
    // collections
    p("push_if", "", Step, &[("collection", &["__si_collection"]), ("condition", &["__si_condition"]), ("value", &["__si_value"])], None),
    p("push_some", "", Step, &[("collection", &["__si_collection"]), ("option", &["__si_option"])], None),
    p("extend_if", "", Step, &[("collection", &["__si_collection"]), ("condition", &["__si_condition"]), ("values", &["__si_values"])], None),
    p("extend_some", "", Step, &[("collection", &["__si_collection"]), ("option", &["__si_option"])], None),
    p("insert_if", "", Step, &[("map", &["__si_map"]), ("condition", &["__si_condition"]), ("key", &["__si_key"]), ("value", &["__si_value"])], None),
    p("insert_some", "", Step, &[("map", &["__si_map"]), ("key", &["__si_key"]), ("option", &["__si_option"])], None),
    p("assign_if", "", Step, &[("condition", &["__si_condition"]), ("place", &["__si_place"]), ("value", &["__si_value"])], None),
    p("assign_if_some", "", Step, &[("place", &["__si_place"]), ("option", &["__si_option"])], None),
    p("push_unique", "", Step, &[("collection", &["__si_collection"]), ("value", &["__si_value"])], None),
    p("extend_unique", "", Step, &[("collection", &["__si_collection"]), ("values", &["__si_values"])], None),
    // argument vectors
    p("sc_cmd", "", Value, &[("value", &["__si_value"])], None),
    p("sc_flags", "1 field", Step, &[("parts", &["__sid_parts"]), ("this", &["__sid_this"]), ("fields", &["__sid_fields_0"]), ("flags", &["__si_flags_0"])], None),
    p("sc_flags_into", "1 field", Step, &[("parts", &["__sid_parts"]), ("this", &["__sid_this"]), ("fields", &["__sid_fields_0"]), ("flags", &["__si_flags_0"])], None),
    p("sc_opt_kv", "1 field", Step, &[("parts", &["__sid_parts"]), ("this", &["__sid_this"]), ("fields", &["__sid_fields_0"]), ("flags", &["__si_flags_0"])], None),
    p("sc_args", "1 argument", Step, &[("parts", &["__sid_parts"]), ("args", &["__si_args_0"])], None),
    p("sc_if_some", "", Step, &[("parts", &["__sid_parts"]), ("opt", &["__si_opt"]), ("map", &["__si_map"])], None),
    p("sc_if_some_display", "", Step, &[("parts", &["__sid_parts"]), ("opt", &["__si_opt"])], None),
    p("sc_if_some_into", "", Step, &[("parts", &["__sid_parts"]), ("opt", &["__si_opt"])], None),
    p("sc_if_some_clone", "", Step, &[("parts", &["__sid_parts"]), ("opt", &["__si_opt"])], None),
    p("sc_if_some_prefix", "", Step, &[("parts", &["__sid_parts"]), ("opt", &["__si_opt"]), ("prefix", &["__si_prefix"])], None),
    p("sc_each", "", Step, &[("parts", &["__sid_parts"]), ("iter", &["__si_iter"]), ("map", &["__si_map"])], None),
    p("sc_each_display", "", Step, &[("parts", &["__sid_parts"]), ("iter", &["__si_iter"])], None),
    p("sc_if_some_flag", "", Step, &[("vec", &["__si_vec"]), ("opt", &["__si_opt"]), ("flag", &["__si_flag"])], None),
    // whole functions from the signature
    p("simple_method", "with a return type", Function, &[("value", &["__si_value"])], sig("", &[("__sia_args", "__SiArgsTy")], Some("__SiRet"))),
    p("simple_method", "no return type", Function, &[("value", &["__si_value"])], sig("", &[("__sia_args", "__SiArgsTy")], None)),
    p("simple_method", "const, with a return type", Function, &[("value", &["__si_value"])], const_sig("", &[("__sia_args", "__SiArgsTy")], Some("__SiRet"))),
    p("checked_method", "1 condition", Function, &[("error_ty", &["__SiErrorTy"]), ("conditions", C1), ("errors", E1), ("value", &["__si_value"])], sig("", &[("__sia_args", "__SiArgsTy")], Some("__SiOk"))),
    p("checked_method", "2 conditions", Function, &[("error_ty", &["__SiErrorTy"]), ("conditions", C2), ("errors", E2), ("value", &["__si_value"])], sig("", &[("__sia_args", "__SiArgsTy")], Some("__SiOk"))),
    p("checked_constructor", "1 condition", Function, &[("error_ty", &["__SiErrorTy"]), ("conditions", C1), ("errors", E1), ("value", &["__si_value"])], sig("", &[("__sia_args", "__SiArgsTy")], None)),
    p("checked_constructor", "2 conditions", Function, &[("error_ty", &["__SiErrorTy"]), ("conditions", C2), ("errors", E2), ("value", &["__si_value"])], sig("", &[("__sia_args", "__SiArgsTy")], None)),
    p("getter", "", Function, SELF_PATH, sig("&self", &[], Some("__SiRet"))),
    p("getter", "const", Function, SELF_PATH, const_sig("&self", &[], Some("__SiRet"))),
    p("getter_ref", "", Function, SELF_PATH, sig("&self", &[], Some("&__SiRet"))),
    p("getter_mut", "", Function, SELF_PATH, sig("&mut self", &[], Some("&mut __SiRet"))),
    p("getter_option_ref", "", Function, SELF_FIELD, sig("&self", &[], Some("Option<&__SiRet>"))),
    p("getter_option_deref", "", Function, SELF_FIELD, sig("&self", &[], Some("Option<&__SiRet>"))),
    p("getter_slice", "", Function, SELF_FIELD, sig("&self", &[], Some("&[__SiElem]"))),
    p("len_field", "", Function, SELF_FIELD, sig("&self", &[], None)),
    p("is_empty_field", "", Function, SELF_FIELD, sig("&self", &[], None)),
    p("bool_field", "", Function, SELF_FIELD, sig("&self", &[], None)),
    p("contains_field", "", Function, SELF_FIELD, sig("&self", &[("__sid_arg", "__SiTy")], None)),
    p("find_by", "", Function, &[("field", &["__sid_field"]), ("item", &["__sid_item"]), ("condition", &["__si_condition"])], sig("&self", &[("__sid_arg", "__SiArgTy")], Some("Option<&__SiItemTy>"))),
    p("delegate", "", Function, &[("field", &["__sid_field"]), ("method", &["__sid_method"])], sig("&self", &[], Some("__SiRet"))),
    p("delegate", "passing an argument", Function, &[("field", &["__sid_field"]), ("method", &["__sid_method"]), ("pass", &["__sid_pass"])], sig("&self", &[("__sid_arg", "__SiTy")], Some("__SiRet"))),
    p("delegate_mut", "", Function, &[("field", &["__sid_field"]), ("method", &["__sid_method"])], sig("&mut self", &[], Some("__SiRet"))),
    p("delegate_mut", "passing an argument", Function, &[("field", &["__sid_field"]), ("method", &["__sid_method"]), ("pass", &["__sid_pass"])], sig("&mut self", &[("__sid_arg", "__SiTy")], Some("__SiRet"))),
    p("newtype_value", "", Function, &[], sig("self", &[], Some("__SiRet"))),
    p("newtype_value", "const", Function, &[], const_sig("self", &[], Some("__SiRet"))),
    p("newtype_ref", "", Function, &[], sig("&self", &[], Some("&__SiRet"))),
    p("setter", "", Function, SELF_PATH, sig("&mut self", &[("__sid_arg", "__SiTy")], None)),
    p("setter_into", "", Function, SELF_PATH, sig("&mut self", &[("__sid_arg", "__SiTy")], None)),
    p("setter_some", "", Function, SELF_FIELD, sig("&mut self", &[("__sid_arg", "__SiTy")], None)),
    p("setter_some_into", "", Function, SELF_FIELD, sig("&mut self", &[("__sid_arg", "__SiTy")], None)),
    p("clear_collection", "", Function, SELF_FIELD, sig("&mut self", &[], None)),
    p("take_field", "", Function, SELF_FIELD, sig("&mut self", &[], Some("Option<__SiTy>"))),
    p("enable_field", "", Function, SELF_FIELD, sig("&mut self", &[], None)),
    p("disable_field", "", Function, SELF_FIELD, sig("&mut self", &[], None)),
    p("toggle_field", "", Function, SELF_FIELD, sig("&mut self", &[], None)),
    p("with_field", "", Function, SELF_PATH, sig("mut self", &[("__sid_arg", "__SiTy")], Some("Self"))),
    p("with_field", "const", Function, SELF_PATH, const_sig("mut self", &[("__sid_arg", "__SiTy")], Some("Self"))),
    p("with_into", "", Function, SELF_FIELD, sig("mut self", &[("__sid_arg", "__SiTy")], Some("Self"))),
    p("with_some", "", Function, SELF_FIELD, sig("mut self", &[("__sid_arg", "__SiTy")], Some("Self"))),
    p("with_some_into", "", Function, SELF_FIELD, sig("mut self", &[("__sid_arg", "__SiTy")], Some("Self"))),
    p("with_push", "", Function, SELF_FIELD, sig("mut self", &[("__sid_arg", "__SiTy")], Some("Self"))),
    p("with_push_into", "", Function, SELF_FIELD, sig("mut self", &[("__sid_arg", "__SiTy")], Some("Self"))),
    p("with_extend", "", Function, &[("field", &["__sid_field"]), ("elem", &["__SiElem"])], sig("mut self", &[("__sid_arg", "")], Some("Self"))),
    p("with_flag", "", Function, SELF_FIELD, sig("mut self", &[], Some("Self"))),
    p("without_flag", "", Function, SELF_FIELD, sig("mut self", &[], Some("Self"))),
    p("with_unique_push", "", Function, SELF_FIELD, sig("mut self", &[("__sid_arg", "__SiTy")], Some("Self"))),
    p("newtype_constructor", "", Function, &[], sig("", &[("__sid_arg", "__SiTy")], Some("Self"))),
    p("newtype_constructor", "const", Function, &[], const_sig("", &[("__sid_arg", "__SiTy")], Some("Self"))),
    p("forward_trait_method", "self", Function, &[("target", &["__sit_target"])], sig("self", &[], Some("__SiRet"))),
    p("forward_trait_method", "self and an argument", Function, &[("target", &["__sit_target"])], sig("self", &[("__sid_arg", "__SiTy")], Some("__SiRet"))),
    p("forward_trait_method", "&self", Function, &[("target", &["__sit_target"])], sig("&self", &[], Some("__SiRet"))),
    p("forward_trait_method", "&self and an argument", Function, &[("target", &["__sit_target"])], sig("&self", &[("__sid_arg", "__SiTy")], Some("__SiRet"))),
    // builder lines (each a whole method)
    p("builder_new_default", "", Function, &[], None),
    p("builder_opt", "", Function, &[("name", &["__sid_name"]), ("field", &["__sid_field"]), ("ty", &["__SiTy"])], None),
    p("builder_opt_into", "", Function, &[("name", &["__sid_name"]), ("field", &["__sid_field"]), ("ty", &["__SiTy"])], None),
    p("builder_set", "", Function, &[("name", &["__sid_name"]), ("field", &["__sid_field"]), ("ty", &["__SiTy"])], None),
    p("builder_vec_into", "", Function, &[("name", &["__sid_name"]), ("field", &["__sid_field"]), ("elem", &["__SiElem"])], None),
    p("builder_vec_iter", "", Function, &[("name", &["__sid_name"]), ("field", &["__sid_field"]), ("elem", &["__SiElem"])], None),
    p("builder_push", "", Function, &[("name", &["__sid_name"]), ("field", &["__sid_field"]), ("elem", &["__SiElem"])], None),
    p("builder_flag", "", Function, &[("name", &["__sid_name"]), ("field", &["__sid_field"])], None),
    // trait impls (the one function each writes)
    p("impl_default_via_new", "", ImplFn, &[("ty", &["__SiTy"])], None),
    p("impl_from_newtype", "", ImplFn, &[("wrapper", &["__SiWrapper"]), ("inner", &["__SiInner"])], None),
    p("impl_as_ref_newtype", "", ImplFn, &[("wrapper", &["__SiWrapper"]), ("inner", &["__SiInner"])], None),
    p("impl_deref_newtype", "", ImplFn, &[("wrapper", &["__SiWrapper"]), ("target", &["__SiTarget"])], None),
    p("impl_deref_mut_newtype", "", ImplFn, &[("wrapper", &["__SiWrapper"])], None),
    p("impl_display_via", "", ImplFn, &[("ty", &["__SiTy"]), ("method", &["__sid_method"])], None),
    p("impl_debug_redacted", "", ImplFn, &[("ty", &["__SiTy"]), ("text", &["__si_text"])], None),
    p("impl_partial_ord_via_ord", "", ImplFn, &[("ty", &["__SiTy"])], None),
    p("impl_from_variant", "", ImplFn, &[("source", &["__SiSource"]), ("target", &["__SiTarget"]), ("variant", &["__sid_variant"])], None),
];

/// The pieces the catalog has no shape for, and why.
const NOT_MATCHED: &[(&str, &str)] = &[
    ("guard_with", "takes a cleanup block: its statements are spliced into the guard, so no one statement has its shape"),
    ("ensure_with", "takes a cleanup block: its statements are spliced into the guard, so no one statement has its shape"),
    ("bail_with", "takes a cleanup block: its statements are spliced into the guard, so no one statement has its shape"),
    ("ensure_all", "expands to one ensure per pair: several statements, each matched as an `ensure`"),
    ("impl_std_error", "writes an impl with no function: there is no body to match"),
    ("shell_command_builder", "writes a whole struct and its impls, not a function"),
    ("define_shell_builder", "writes a whole struct and its impls, not a function"),
];

/// A probe's piece, its slots filled with the placeholders.
fn piece_of(pr: &Probe) -> Piece {
    let mut p = Piece { piece: pr.piece.to_string(), ..Piece::default() };
    for (slot, values) in pr.slots {
        let one = Some(values[0].to_string());
        let many: Vec<String> = values.iter().map(|v| v.to_string()).collect();
        match *slot {
            "condition" => p.condition = one,
            "conditions" => p.conditions = many,
            "error" => p.error = one,
            "errors" => p.errors = many,
            "value" => p.value = one,
            "values" => p.values = one,
            "option" => p.option = one,
            "result" => p.result = one,
            "map_error" => p.map_error = one,
            "return_value" => p.return_value = one,
            "left" => p.left = one,
            "right" => p.right = one,
            "expected" => p.expected = one,
            "minimum" => p.minimum = one,
            "maximum" => p.maximum = one,
            "collection" => p.collection = one,
            "map" => p.map = one,
            "key" => p.key = one,
            "place" => p.place = one,
            "candidates" => p.candidates = many,
            "parts" => p.parts = one,
            "this" => p.this = one,
            "fields" => p.fields = many,
            "flags" => p.flags = many,
            "args" => p.args = many,
            "opt" => p.opt = one,
            "prefix" => p.prefix = one,
            "iter" => p.iter = one,
            "vec" => p.vec = one,
            "flag" => p.flag = one,
            "field" => p.field = one,
            "method" => p.method = one,
            "pass" => p.pass = one,
            "item" => p.item = one,
            "target" => p.target = one,
            "elem" => p.elem = one,
            "error_ty" => p.error_ty = one,
            "ty" => p.ty = one,
            "wrapper" => p.wrapper = one,
            "inner" => p.inner = one,
            "text" => p.text = one,
            "source" => p.source = one,
            "variant" => p.variant = one,
            "name" => p.name = one,
            other => panic!("probe slot {other} is not a slot of Piece"),
        }
    }
    p
}

/// A probe's function spec (a method piece's generator on its signature).
fn fn_spec_of(pr: &Probe) -> FnSpec {
    let s = pr.sig.unwrap_or(Sig { receiver: "", params: &[], ret: None, is_const: false });
    FnSpec {
        name: "__sid_name".into(),
        receiver: (!s.receiver.is_empty()).then(|| s.receiver.to_string()),
        params: s.params.iter().map(|(n, t)| Param { name: n.to_string(), ty: t.to_string() }).collect(),
        ret: s.ret.map(str::to_string),
        is_const: s.is_const,
        generator: Some(piece_of(pr)),
        ..FnSpec::default()
    }
}

/// The call a probe makes, as the change vocabulary writes it.
pub fn call_of(index: usize) -> String {
    let pr = &PROBES[index];
    match pr.position {
        Step | Value => change::render_step(&piece_of(pr)).expect("a probe's piece renders"),
        Function => change::render_fn(&fn_spec_of(pr), "").expect("a probe's function renders").trim_end().to_string(),
        ImplFn => change::render_impl_piece(&piece_of(pr)).expect("a probe's impl renders").trim_end().to_string(),
    }
}

/// The probe crate's `lib.rs`: each call in an item of its own, named by its index.
pub fn probe_source() -> String {
    let mut s = String::from("// The toolkit's probe: one call of each piece on placeholders (custodian_kit::toolkit::probe).\n#![allow(unused, non_camel_case_types, non_snake_case)]\n\n");
    for (i, pr) in PROBES.iter().enumerate() {
        let call = call_of(i);
        match pr.position {
            Step => s.push_str(&format!("fn __si_probe_{i}() {{\n    {call}\n}}\n\n")),
            Value => s.push_str(&format!("fn __si_probe_{i}() {{\n    {}\n}}\n\n", call.trim_end_matches(';'))),
            Function => s.push_str(&format!("struct __SiProbe{i};\nimpl __SiProbe{i} {{\n    {call}\n}}\n\n")),
            ImplFn => s.push_str(&format!("mod __si_probe_{i} {{\n    {call}\n}}\n\n")),
        }
    }
    s
}

/// Writes the probe crate under `dir` (a directory of its own): `Cargo.toml` naming the toolkit, `src/lib.rs`.
pub fn write_probe_crate(dir: &Path) -> std::io::Result<()> {
    write_crate(dir, &probe_source())
}

fn write_crate(dir: &Path, lib: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(dir.join("src"))?;
    std::fs::write(
        dir.join("Cargo.toml"),
        format!("[package]\nname = \"toolkit_probe\"\nversion = \"0.0.0\"\nedition = \"2021\"\npublish = false\n\n[dependencies]\n{} = \"={}\"\n\n[workspace]\n", TOOLKIT.0, TOOLKIT.1),
    )?;
    std::fs::write(dir.join("src/lib.rs"), lib)?;
    // klepto reads a dependency's macros through cargo's lockfile
    crate::exec::lock_crate(dir).map_err(|e| std::io::Error::other(e.to_string()))
}

/// Puts a std expression macro (`vec!`, `format!`, `write!`) back as written where klepto reads it as the tuple of
/// its arguments: hand-written code holds the macro, so the shape must.
struct Restore;

impl VisitMut for Restore {
    fn visit_expr_mut(&mut self, e: &mut syn::Expr) {
        let attrs = klepto::resolve::expand::expr_attrs(e);
        if let (Some((path, _)), Some(tokens)) = (klepto::resolve::expand::invoked_by(attrs), klepto::resolve::expand::invoked_tokens(attrs)) {
            if klepto::resolve::expand::is_expression_macro(&path) {
                if let (Ok(p), Ok(tokens)) = (syn::parse_str::<syn::Path>(&path), tokens.parse::<proc_macro2::TokenStream>()) {
                    let delimiter = if path.ends_with("vec") { syn::MacroDelimiter::Bracket(Default::default()) } else { syn::MacroDelimiter::Paren(Default::default()) };
                    *e = syn::Expr::Macro(syn::ExprMacro { attrs: Vec::new(), mac: syn::Macro { path: p, bang_token: Default::default(), delimiter, tokens } });
                    return;
                }
            }
        }
        syn::visit_mut::visit_expr_mut(self, e);
    }
}

/// The probe crate's file as klepto expanded it: std expression macros as written, klepto's markers off.
fn expanded_file(k: &klepto::Klepto) -> Result<syn::File, String> {
    let pf = k.files.iter().find(|f| f.path.ends_with("src/lib.rs")).ok_or("the probe crate's lib.rs was not scanned")?;
    let mut file = pf.ast.clone();
    Restore.visit_file_mut(&mut file);
    Unmark.visit_file_mut(&mut file);
    Ok(file)
}

/// The probe crate written under `dir` and expanded until no toolkit call is left: klepto expands a statement's
/// call one level per scan (a call an expansion makes, `$crate::ensure!` inside `ensure_eq!`, is left as a call),
/// so each round's expansion is written as the next round's crate (`dir/round<n>`) and scanned again.
pub fn expand_probe(dir: &Path) -> Result<syn::File, String> {
    write_probe_crate(dir).map_err(|e| e.to_string())?;
    let mut file = expanded_file(&scan_probe(&dir.canonicalize().map_err(|e| e.to_string())?)?)?;
    for round in 2..=6 {
        let text = file.to_token_stream().to_string();
        if !text.contains(&format!("{} ::", TOOLKIT.0)) {
            return Ok(file);
        }
        let next = dir.join(format!("round{round}"));
        write_crate(&next, &text).map_err(|e| e.to_string())?;
        file = expanded_file(&scan_probe(&next.canonicalize().map_err(|e| e.to_string())?)?)?;
    }
    Err("toolkit calls are left after six rounds of expansion".into())
}

/// Takes klepto's markers off every node.
struct Unmark;

impl VisitMut for Unmark {
    fn visit_attributes_mut(&mut self, attrs: &mut Vec<syn::Attribute>) {
        attrs.retain(|a| !klepto::resolve::expand::is_marker(a));
        for a in attrs.iter_mut() {
            self.visit_attribute_mut(a);
        }
    }
}

/// Every name an expansion binds that is not a placeholder (`value` in `Some(value) => value`, a builder's
/// `value` parameter, `f` in a `fmt`), in first-bound order.
#[derive(Default)]
struct Bound {
    names: Vec<String>,
}

impl<'ast> Visit<'ast> for Bound {
    fn visit_pat_ident(&mut self, p: &'ast syn::PatIdent) {
        let n = p.ident.to_string();
        if HoleKind::of(&n).is_none() && n != "self" && !self.names.contains(&n) {
            self.names.push(n);
        }
        syn::visit::visit_pat_ident(self, p);
    }
}

/// The one function an expansion holds: the probe fn's own body (a step or a value), the method in the probe's
/// impl, the function of the impl a piece wrote.
enum Found {
    Body(syn::Block),
    Method(syn::ImplItemFn),
    TraitFn { trait_name: String, f: syn::ImplItemFn },
}

fn found_in(file: &syn::File, index: usize) -> Option<Found> {
    let probe_fn = format!("__si_probe_{index}");
    let probe_ty = format!("__SiProbe{index}");
    for item in &file.items {
        match item {
            syn::Item::Fn(f) if f.sig.ident == probe_fn => return Some(Found::Body((*f.block).clone())),
            syn::Item::Impl(i) if i.self_ty.to_token_stream().to_string() == probe_ty => {
                return i.items.iter().find_map(|x| match x {
                    syn::ImplItem::Fn(f) => Some(Found::Method(f.clone())),
                    _ => None,
                })
            }
            syn::Item::Mod(m) if m.ident == probe_fn => {
                for it in m.content.as_ref().map(|(_, v)| v.as_slice()).unwrap_or(&[]) {
                    if let syn::Item::Impl(i) = it {
                        let trait_name = i.trait_.as_ref().and_then(|(_, p, _)| p.segments.last()).map(|s| s.ident.to_string())?;
                        return i.items.iter().find_map(|x| match x {
                            syn::ImplItem::Fn(f) => Some(Found::TraitFn { trait_name: trait_name.clone(), f: f.clone() }),
                            _ => None,
                        });
                    }
                }
            }
            _ => {}
        }
    }
    None
}

/// The tokens of what a probe's call expanded to, unmarked: a step's statements, a value's expression, a function.
fn expansion_tokens(found: &Found, position: Position) -> Option<proc_macro2::TokenStream> {
    let mut ts = proc_macro2::TokenStream::new();
    match (found, position) {
        (Found::Body(b), Step) => {
            for s in &b.stmts {
                s.to_tokens(&mut ts);
            }
        }
        // the probe's one statement: klepto writes a std expression macro's statement with a `;` it reads as a value
        (Found::Body(b), Value) => match b.stmts.last()? {
            syn::Stmt::Expr(e, _) => e.to_tokens(&mut ts),
            syn::Stmt::Macro(m) => m.mac.to_tokens(&mut ts),
            _ => return None,
        },
        (Found::Method(f), Function) | (Found::TraitFn { f, .. }, ImplFn) => f.to_tokens(&mut ts),
        _ => return None,
    }
    Some(ts)
}

/// What probe `index`'s call expanded to in an expanded probe file (cargo expand's output): its tokens.
pub fn expansion_in(file: &syn::File, index: usize, position: Position) -> Option<proc_macro2::TokenStream> {
    expansion_tokens(&found_in(file, index)?, position)
}

/// Every hole a pattern holds, by its placeholder's spelling, with the slot each fills.
fn holes_of(pattern: &str) -> Vec<Hole> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for word in pattern.split(|c: char| !(c.is_alphanumeric() || c == '_')) {
        let Some(kind) = HoleKind::of(word) else { continue };
        if !seen.insert(word.to_string()) {
            continue;
        }
        out.push(Hole { placeholder: word.to_string(), slot: slot_of(word), kind });
    }
    out
}

/// The slot a placeholder fills: `__si_conditions_1` → `conditions`, `__SiErrorTy` → `error_ty`, `__sid_name` →
/// `name`; a binder fills none.
pub fn slot_of(placeholder: &str) -> String {
    let rest = ["__sib_", "__sid_", "__sif_", "__sip_", "__sit_", "__sia_", "__si_"].iter().find_map(|p| placeholder.strip_prefix(p));
    let base = match rest {
        Some(r) => r.to_string(),
        None => {
            // a type: CamelCase to snake_case
            let r = placeholder.trim_start_matches("__Si");
            let mut s = String::new();
            for (i, c) in r.chars().enumerate() {
                if c.is_uppercase() {
                    if i > 0 {
                        s.push('_');
                    }
                    s.extend(c.to_lowercase());
                } else {
                    s.push(c);
                }
            }
            s
        }
    };
    // a list slot's element: `conditions_1` → `conditions`
    match base.rsplit_once('_') {
        Some((head, n)) if n.chars().all(|c| c.is_ascii_digit()) && !n.is_empty() => head.to_string(),
        _ => base,
    }
}

/// Whether an expansion holds a std macro (`vec!`, `format!`, `write!`, `matches!`), which cargo expand opens into
/// std's insides and hand-written code holds as the macro.
pub fn holds_std_macro(expansion: &str) -> bool {
    ["vec !", "format !", "write !", "matches !"].iter().any(|m| expansion.contains(m))
}

/// Probe `i`'s shape from an expanded probe file.
fn shape_in(file: &syn::File, i: usize) -> Result<ToolkitShape, String> {
    let pr = &PROBES[i];
    let found = found_in(file, i).ok_or_else(|| format!("probe {i} ({} {}): nothing expanded", pr.piece, pr.variant))?;
    let trait_name = match &found {
        Found::TraitFn { trait_name, .. } => Some(trait_name.clone()),
        _ => None,
    };
    let ts = expansion_tokens(&found, pr.position).ok_or_else(|| format!("probe {i} ({} {}): not the expected form", pr.piece, pr.variant))?;
    let expansion = ts.to_string();
    if expansion.contains(&format!("{} ::", TOOLKIT.0)) {
        return Err(format!("probe {i} ({} {}): a toolkit call is left unexpanded: {expansion}", pr.piece, pr.variant));
    }
    // the names the expansion binds itself become binders
    let mut bound = Bound::default();
    match &found {
        Found::Body(b) => bound.visit_block(b),
        Found::Method(f) | Found::TraitFn { f, .. } => bound.visit_impl_item_fn(f),
    }
    let binders: Vec<(String, String)> = bound.names.iter().enumerate().map(|(n, name)| (format!("__sib_{n}"), name.clone())).collect();
    let normal = normal::normal_tokens(ts, pr.position, Level::Exact).map_err(|e| format!("probe {i} ({} {}): {e}", pr.piece, pr.variant))?;
    let mut pattern = normal.text;
    for (ph, name) in &binders {
        pattern = rename_word(&pattern, name, ph);
    }
    // `($($args:tt)*)`: the parameters as written are one hole
    let pattern = pattern.replace("__sia_args : __SiArgsTy", "__sia_args");
    let expansion = expansion.replace("__sia_args : __SiArgsTy", "__sia_args");
    let holes = holes_of(&pattern);
    let specificity = pattern.split_whitespace().filter(|w| HoleKind::of(w).is_none()).count();
    Ok(ToolkitShape {
        piece: pr.piece.to_string(),
        family: change::piece(pr.piece).map(|d| d.family).ok_or_else(|| format!("{} is not a piece", pr.piece))?,
        variant: pr.variant.to_string(),
        position: pr.position,
        call: call_of(i),
        expansion,
        pattern,
        holes,
        binders,
        trait_name,
        specificity,
        expanded_by: String::new(),
        klepto_same: None,
    })
}

/// The catalog: each shape from cargo expand's expansion of the probe (rustc's own), unless it holds a std macro
/// cargo expand opens, when it is klepto's (std's macros as written); `klepto_same` says whether klepto, which the
/// reading reads the workspace with, expands it to the same normal form.
pub fn catalog_from(klepto_file: &syn::File, cargo_file: Option<&syn::File>) -> Result<Catalog, String> {
    let mut shapes = Vec::new();
    for i in 0..PROBES.len() {
        let k = shape_in(klepto_file, i)?;
        let shape = match cargo_file {
            Some(cf) if !holds_std_macro(&k.expansion) => {
                let c = shape_in(cf, i)?;
                let same = c.pattern == k.pattern;
                ToolkitShape { expanded_by: "cargo expand".into(), klepto_same: Some(same), ..c }
            }
            _ => ToolkitShape { expanded_by: "klepto (it holds a std macro, which cargo expand opens)".into(), ..k },
        };
        shapes.push(shape);
    }
    Ok(Catalog {
        schema: SHAPES_SCHEMA.to_string(),
        toolkit: format!("{} {}", TOOLKIT.0, TOOLKIT.1),
        shapes,
        not_matched: NOT_MATCHED.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect(),
    })
}

/// The catalog made from nothing but the probe: written under `dir` (a new directory), expanded by klepto in
/// rounds and by cargo expand (a declared line).
pub fn build_catalog(dir: &Path) -> Result<Catalog, String> {
    let k = expand_probe(dir)?;
    let expanded = crate::exec::expand_crate(dir, "toolkit_probe").map_err(|e| e.to_string())?;
    let c = syn::parse_file(&expanded).map_err(|e| format!("cargo expand's output does not parse: {e}"))?;
    catalog_from(&k, Some(&c))
}

/// `old` replaced by `new` where it is a whole word.
pub fn rename_word(text: &str, old: &str, new: &str) -> String {
    let word = |c: Option<char>| c.is_some_and(|c| c.is_alphanumeric() || c == '_');
    let mut out = String::new();
    let mut i = 0;
    while let Some(pos) = text[i..].find(old) {
        let at = i + pos;
        let (before, after) = (text[..at].chars().last(), text[at + old.len()..].chars().next());
        out.push_str(&text[i..at]);
        out.push_str(if !word(before) && !word(after) { new } else { old });
        i = at + old.len();
    }
    out.push_str(&text[i..]);
    out
}

/// The probe crate scanned by klepto (dependency macros on: the toolkit is a dependency).
pub fn scan_probe(dir: &Path) -> Result<klepto::Klepto, String> {
    let opts = crate::reading::spec::Scope::default().scan_options();
    crate::klepto_scan(dir, &opts).map_err(|e| e.to_string())
}

/// Every piece of the change vocabulary the catalog covers or says why not (the test's check).
pub fn pieces_accounted() -> (Vec<String>, Vec<String>) {
    let probed: BTreeSet<&str> = PROBES.iter().map(|p| p.piece).collect();
    let left: BTreeSet<&str> = NOT_MATCHED.iter().map(|(p, _)| *p).collect();
    let all: Vec<String> = change::PIECES.iter().filter(|p| p.family != change::Family::Step).map(|p| p.name.to_string()).collect();
    let missing = all.iter().filter(|p| !probed.contains(p.as_str()) && !left.contains(p.as_str())).cloned().collect();
    let both = all.iter().filter(|p| probed.contains(p.as_str()) && left.contains(p.as_str())).cloned().collect();
    (missing, both)
}

/// The text a probe's slot holds around one of its placeholders, when the slot is more than the placeholder:
/// `require_some_into`'s `ok_or_else` call takes `|| __si_error`, so the code a body holds there (`|| make()`)
/// is written back into the call as the closure, never as the bare `make()` (which is the `ok_or` call).
pub fn written_in(index: usize, placeholder: &str) -> Option<&'static str> {
    PROBES.get(index)?.slots.iter().flat_map(|(_, texts)| texts.iter().copied()).find(|t| *t != placeholder && rename_word(t, placeholder, "\u{0}") != *t)
}

/// How many calls the probe makes.
pub fn probe_count() -> usize {
    PROBES.len()
}
