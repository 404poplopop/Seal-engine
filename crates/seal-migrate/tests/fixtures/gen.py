#!/usr/bin/env python3
"""Writes the copy-paste export fixtures in this folder.

The fixtures are hand-designed graphs written in the exact text format the
classic editor puts on the clipboard, so the importer is tested on the real
syntax. Run: python3 gen.py
"""
import hashlib, os

HERE = os.path.dirname(os.path.abspath(__file__))
BOOL, INT, REAL, STR, EXEC = "bool", "int", "real", "string", "exec"

def guid(*parts):
    return hashlib.md5("/".join(parts).encode()).hexdigest().upper()

class N:
    def __init__(self, cls, name, props=(), pins=()):
        self.cls, self.name, self.props = cls, name, list(props)
        # pins: (name, dir 'in'/'out', category, default)
        self.pins = [list(p) + [None] * (4 - len(p)) for p in pins]
        self.links = {p[0]: [] for p in self.pins}

def link(a, ap, b, bp):
    a.links[ap].append((b.name, guid(b.name, bp)))
    b.links[bp].append((a.name, guid(a.name, ap)))

def pin_line(n, p):
    name, d, cat, default = p
    container = "None"
    if cat.endswith("[]"):
        cat, container = cat[:-2], "Array"
    sub = "double" if cat == REAL else ""
    obj = "\"/Script/CoreUObject.ScriptStruct'/Script/CoreUObject.Vector2D'\"" if cat == "struct" else "None"
    if cat == "vector":
        cat, obj = "struct", "\"/Script/CoreUObject.ScriptStruct'/Script/CoreUObject.Vector'\""
    if cat == "transform":
        cat, obj = "struct", "\"/Script/CoreUObject.ScriptStruct'/Script/CoreUObject.Transform'\""
    if cat == "enum":
        cat, obj = "byte", "\"/Script/Engine.UserDefinedEnum'/Game/Mood.Mood'\""
    if cat == "timerhandle":
        cat, obj = "struct", "\"/Script/CoreUObject.ScriptStruct'/Script/Engine.TimerHandle'\""
    if cat == "rotator":
        cat, obj = "struct", "\"/Script/CoreUObject.ScriptStruct'/Script/CoreUObject.Rotator'\""
    parts = [f"PinId={guid(n.name, name)}", f'PinName="{name}"']
    if d == "out":
        parts.append('Direction="EGPD_Output"')
    parts += [f'PinType.PinCategory="{cat}"', f'PinType.PinSubCategory="{sub}"', f"PinType.PinSubCategoryObject={obj}",
              "PinType.PinSubCategoryMemberReference=()", "PinType.PinValueType=()", f"PinType.ContainerType={container}",
              "PinType.bIsReference=False", "PinType.bIsConst=False", "PinType.bIsWeakPointer=False",
              "PinType.bIsUObjectWrapper=False", "PinType.bSerializeAsSinglePrecisionFloat=False"]
    if default is not None and str(default).startswith("obj:"):
        parts.append(f'DefaultObject="{default[4:]}"')
    elif default is not None and str(default).startswith("text:"):
        parts.append(f"DefaultTextValue={default[5:]}")
    elif default is not None:
        parts.append(f'DefaultValue="{default}"')
    if n.links[name]:
        parts.append("LinkedTo=(" + "".join(f"{a} {b}," for a, b in n.links[name]) + ")")
    parts += ["PersistentGuid=00000000000000000000000000000000", "bHidden=" + ("True" if name in ("self", "WorldContextObject") else "False"),
              "bNotConnectable=False", "bDefaultValueIsReadOnly=False", "bDefaultValueIsIgnored=False",
              "bAdvancedView=False", "bOrphanedPin=False"]
    return "   CustomProperties Pin (" + ",".join(parts) + ",)"

def write(path, nodes):
    out = []
    for i, n in enumerate(nodes):
        out.append(f'Begin Object Class={n.cls} Name="{n.name}" ExportPath="/Script/BlueprintGraph.{n.name}"')
        for p in n.props:
            out.append("   " + p)
        out += [f"   NodePosX={i * 240}", "   NodePosY=0", f"   NodeGuid={guid(n.name, 'node')}"]
        out += [pin_line(n, p) for p in n.pins]
        out.append("End Object")
    with open(os.path.join(HERE, path), "w", newline="\n") as f:
        f.write("\n".join(out) + "\n")

G = "/Script/BlueprintGraph."
MATH = "MemberParent=/Script/CoreUObject.Class'\"/Script/Engine.KismetMathLibrary\"'"
SYS = "MemberParent=/Script/CoreUObject.Class'\"/Script/Engine.KismetSystemLibrary\"'"
STRL = "MemberParent=/Script/CoreUObject.Class'\"/Script/Engine.KismetStringLibrary\"'"

def event(name, member, extra=()):
    return N(G + "K2Node_Event", name, [f"EventReference=(MemberParent=/Script/CoreUObject.Class'\"/Script/Engine.Actor\"',MemberName=\"{member}\")", "bOverrideFunction=True"],
             [("OutputDelegate", "out", "delegate"), ("then", "out", EXEC), *extra])

def custom(name, fname, params=()):
    return N(G + "K2Node_CustomEvent", name, [f'CustomFunctionName="{fname}"'],
             [("OutputDelegate", "out", "delegate"), ("then", "out", EXEC), *[(p, "out", t) for p, t in params]])

def call(name, lib, member, pins, pure=True):
    ex = [] if pure else [("execute", "in", EXEC), ("then", "out", EXEC)]
    props = [f'FunctionReference=({lib},MemberName="{member}")']
    if pure:
        props.append("bDefaultsToPureFunc=True")
    return N(G + "K2Node_CallFunction", name, props, [*ex, ("self", "in", "object"), *pins])

def print_(name, text=""):
    return call(name, SYS, "PrintString", [("WorldContextObject", "in", "object"), ("InString", "in", STR, text),
                ("bPrintToScreen", "in", BOOL, "true"), ("bPrintToLog", "in", BOOL, "true"), ("Duration", "in", REAL, "2.000000")], pure=False)

def math(name, member, ty, out=None, a="0", b="0"):
    return call(name, MATH, member, [("A", "in", ty, a), ("B", "in", ty, b), ("ReturnValue", "out", out or ty)])

def var_get(name, var, ty):
    return N(G + "K2Node_VariableGet", name, [f'VariableReference=(MemberName="{var}",bSelfContext=True)'], [(var, "out", ty), ("self", "in", "object")])

def var_set(name, var, ty, default=None):
    return N(G + "K2Node_VariableSet", name, [f'VariableReference=(MemberName="{var}",bSelfContext=True)'],
             [("execute", "in", EXEC), ("then", "out", EXEC), (var, "in", ty, default), ("Output_Get", "out", ty), ("self", "in", "object")])

# --- score.txt: events, delay, custom events and calls, variables, math, branch,
# a reroute knot, a sequence, a for loop, a comment and one unsupported node.
begin = event("K2Node_Event_0", "ReceiveBeginPlay")
ready = print_("K2Node_CallFunction_0", "ready")
delay = call("K2Node_CallFunction_1", SYS, "Delay", [("WorldContextObject", "in", "object"), ("Duration", "in", REAL, "0.500000"), ("LatentInfo", "in", "struct")], pure=False)
late = print_("K2Node_CallFunction_2", "late")
call_count = N(G + "K2Node_CallFunction", "K2Node_CallFunction_3", ['FunctionReference=(MemberName="Count To",bSelfContext=True)'],
               [("execute", "in", EXEC), ("then", "out", EXEC), ("self", "in", "object"), ("N", "in", INT, "4")])
add_points = custom("K2Node_CustomEvent_0", "AddPoints", [("Amount", INT)])
get_score = var_get("K2Node_VariableGet_0", "Score", INT)
add = math("K2Node_CallFunction_4", "Add_IntInt", INT)
set_score = var_set("K2Node_VariableSet_0", "Score", INT)
ge = math("K2Node_CallFunction_5", "GreaterEqual_IntInt", INT, BOOL, b="10")
get_won = var_get("K2Node_VariableGet_1", "bWon", BOOL)
not_won = call("K2Node_CallFunction_6", MATH, "Not_PreBool", [("A", "in", BOOL, "false"), ("ReturnValue", "out", BOOL)])
both = math("K2Node_CallFunction_7", "BooleanAND", BOOL, a="false", b="false")
branch = N(G + "K2Node_IfThenElse", "K2Node_IfThenElse_0", [], [("execute", "in", EXEC), ("Condition", "in", BOOL, "true"), ("then", "out", EXEC), ("else", "out", EXEC)])
set_won = var_set("K2Node_VariableSet_1", "bWon", BOOL, "true")
to_str = call("K2Node_CallFunction_8", MATH, "Conv_IntToString", [("InInt", "in", INT, "0"), ("ReturnValue", "out", STR)])
concat = call("K2Node_CallFunction_9", STRL, "Concat_StrStr", [("A", "in", STR, "won at "), ("B", "in", STR, ""), ("ReturnValue", "out", STR)])
knot = N(G + "K2Node_Knot", "K2Node_Knot_0", [], [("InputPin", "in", STR), ("OutputPin", "out", STR)])
won_print = print_("K2Node_CallFunction_10")
spawn = N(G + "K2Node_SpawnActorFromClass", "K2Node_SpawnActorFromClass_0", [], [("execute", "in", EXEC), ("then", "out", EXEC), ("Class", "in", "class")])
tick = event("K2Node_Event_1", "ReceiveTick", [("DeltaSeconds", "out", REAL)])
get_el = var_get("K2Node_VariableGet_2", "Elapsed Time", REAL)
add_el = math("K2Node_CallFunction_11", "Add_DoubleDouble", REAL, a="0.0", b="0.0")
set_el = var_set("K2Node_VariableSet_2", "Elapsed Time", REAL)
count_to = custom("K2Node_CustomEvent_1", "Count To", [("N", INT)])
seq = N(G + "K2Node_ExecutionSequence", "K2Node_ExecutionSequence_0", [], [("execute", "in", EXEC), ("then_0", "out", EXEC), ("then_1", "out", EXEC)])
loop = N(G + "K2Node_MacroInstance", "K2Node_MacroInstance_0",
         ["MacroGraphReference=(MacroGraph=/Script/Engine.EdGraph'\"/Engine/EditorBlueprintResources/StandardMacros.StandardMacros:ForLoop\"',GraphBlueprint=/Script/Engine.Blueprint'\"/Engine/EditorBlueprintResources/StandardMacros.StandardMacros\"')"],
         [("Exec", "in", EXEC), ("FirstIndex", "in", INT, "1"), ("LastIndex", "in", INT, "0"), ("LoopBody", "out", EXEC), ("Index", "out", INT), ("Completed", "out", EXEC)])
get_sum = var_get("K2Node_VariableGet_3", "Sum", INT)
add_sum = math("K2Node_CallFunction_12", "Add_IntInt", INT)
set_sum = var_set("K2Node_VariableSet_3", "Sum", INT)
get_sum2 = var_get("K2Node_VariableGet_4", "Sum", INT)
sum_str = call("K2Node_CallFunction_13", MATH, "Conv_IntToString", [("InInt", "in", INT, "0"), ("ReturnValue", "out", STR)])
sum_print = print_("K2Node_CallFunction_14")
comment = N("/Script/EditorGraph.EdGraphNode_Comment", "EdGraphNode_Comment_0", ['NodeComment="Scoring"'], [])

link(begin, "then", ready, "execute"); link(ready, "then", delay, "execute"); link(delay, "then", late, "execute")
link(late, "then", call_count, "execute")
link(add_points, "then", set_score, "execute"); link(get_score, "Score", add, "A"); link(add_points, "Amount", add, "B")
link(add, "ReturnValue", set_score, "Score"); link(set_score, "then", branch, "execute")
link(set_score, "Output_Get", ge, "A"); link(get_won, "bWon", not_won, "A")
link(ge, "ReturnValue", both, "A"); link(not_won, "ReturnValue", both, "B"); link(both, "ReturnValue", branch, "Condition")
link(branch, "then", set_won, "execute"); link(set_won, "then", won_print, "execute")
link(get_score, "Score", to_str, "InInt"); link(to_str, "ReturnValue", concat, "B")
link(concat, "ReturnValue", knot, "InputPin"); link(knot, "OutputPin", won_print, "InString")
link(branch, "else", spawn, "execute")
link(tick, "then", set_el, "execute"); link(get_el, "Elapsed Time", add_el, "A"); link(tick, "DeltaSeconds", add_el, "B")
link(add_el, "ReturnValue", set_el, "Elapsed Time")
link(count_to, "then", seq, "execute"); link(seq, "then_0", loop, "Exec"); link(count_to, "N", loop, "LastIndex")
link(loop, "LoopBody", set_sum, "execute"); link(get_sum, "Sum", add_sum, "A"); link(loop, "Index", add_sum, "B")
link(add_sum, "ReturnValue", set_sum, "Sum")
link(seq, "then_1", sum_print, "execute"); link(get_sum2, "Sum", sum_str, "InInt"); link(sum_str, "ReturnValue", sum_print, "InString")

write("score.txt", [begin, ready, delay, late, call_count, add_points, get_score, add, set_score, ge, get_won, not_won, both,
                    branch, set_won, to_str, concat, knot, won_print, spawn, tick, get_el, add_el, set_el, count_to, seq, loop,
                    get_sum, add_sum, set_sum, get_sum2, sum_str, sum_print, comment])

# --- flow.txt (event graph) + greet.txt and double.txt (function graphs):
# Do Once, Flip Flop and Gate macros, an add with an extra input, and calls to
# functions copied from their own graphs.
def macro(name, which, pins):
    return N(G + "K2Node_MacroInstance", name,
             [f"MacroGraphReference=(MacroGraph=/Script/Engine.EdGraph'\"/Engine/EditorBlueprintResources/StandardMacros.StandardMacros:{which}\"',GraphBlueprint=/Script/Engine.Blueprint'\"/Engine/EditorBlueprintResources/StandardMacros.StandardMacros\"')"],
             pins)

def self_call(name, member, pins):
    return N(G + "K2Node_CallFunction", name, [f'FunctionReference=(MemberName="{member}",bSelfContext=True)'],
             [("execute", "in", EXEC), ("then", "out", EXEC), ("self", "in", "object"), *pins])

ping = custom("K2Node_CustomEvent_0", "Ping")
rearm = custom("K2Node_CustomEvent_1", "Rearm")
once = macro("K2Node_MacroInstance_0", "DoOnce", [("execute", "in", EXEC), ("Reset", "in", EXEC), ("Start Closed", "in", BOOL, "false"), ("Completed", "out", EXEC)])
once_print = print_("K2Node_CallFunction_0", "once")
flip = custom("K2Node_CustomEvent_2", "Flip")
ff = macro("K2Node_MacroInstance_1", "FlipFlop", [("execute", "in", EXEC), ("A", "out", EXEC), ("B", "out", EXEC), ("Is A", "out", BOOL)])
is_a = call("K2Node_CallFunction_1", STRL, "Conv_BoolToString", [("InBool", "in", BOOL, "false"), ("ReturnValue", "out", STR)])
a_print = print_("K2Node_CallFunction_2")
b_print = print_("K2Node_CallFunction_3", "B")
enter = custom("K2Node_CustomEvent_3", "Enter")
open_ = custom("K2Node_CustomEvent_4", "OpenGate")
toggle = custom("K2Node_CustomEvent_5", "ToggleGate")
gate = macro("K2Node_MacroInstance_2", "Gate", [("Enter", "in", EXEC), ("Open", "in", EXEC), ("Close", "in", EXEC), ("Toggle", "in", EXEC),
                                                ("Start Closed", "in", BOOL, "true"), ("Exit", "out", EXEC)])
through = print_("K2Node_CallFunction_4", "through")
sum3 = custom("K2Node_CustomEvent_6", "Sum3", [("X", INT), ("Y", INT), ("Z", INT)])
add3 = N(G + "K2Node_CommutativeAssociativeBinaryOperator", "K2Node_CommutativeAssociativeBinaryOperator_0",
         [f'FunctionReference=({MATH},MemberName="Add_IntInt")', "bDefaultsToPureFunc=True", "NumAdditionalInputs=1"],
         [("self", "in", "object"), ("A", "in", INT, "0"), ("B", "in", INT, "0"), ("C", "in", INT, "0"), ("ReturnValue", "out", INT)])
sum_str = call("K2Node_CallFunction_5", MATH, "Conv_IntToString", [("InInt", "in", INT, "0"), ("ReturnValue", "out", STR)])
sum_print = print_("K2Node_CallFunction_6")
hello = custom("K2Node_CustomEvent_7", "Hello")
greet_call = self_call("K2Node_CallFunction_7", "Greet", [("Who", "in", STR, "Seal")])
use_double = custom("K2Node_CustomEvent_8", "UseDouble")
double_call = self_call("K2Node_CallFunction_8", "Double", [("X", "in", INT, "2"), ("ReturnValue", "out", INT)])

link(ping, "then", once, "execute"); link(once, "Completed", once_print, "execute"); link(rearm, "then", once, "Reset")
link(flip, "then", ff, "execute"); link(ff, "A", a_print, "execute"); link(ff, "Is A", is_a, "InBool")
link(is_a, "ReturnValue", a_print, "InString"); link(ff, "B", b_print, "execute")
link(enter, "then", gate, "Enter"); link(open_, "then", gate, "Open"); link(toggle, "then", gate, "Toggle")
link(gate, "Exit", through, "execute")
link(sum3, "then", sum_print, "execute"); link(sum3, "X", add3, "A"); link(sum3, "Y", add3, "B"); link(sum3, "Z", add3, "C")
link(add3, "ReturnValue", sum_str, "InInt"); link(sum_str, "ReturnValue", sum_print, "InString")
double_str = call("K2Node_CallFunction_9", MATH, "Conv_IntToString", [("InInt", "in", INT, "0"), ("ReturnValue", "out", STR)])
double_print = print_("K2Node_CallFunction_10")
link(hello, "then", greet_call, "execute"); link(use_double, "then", double_call, "execute")
link(double_call, "then", double_print, "execute"); link(double_call, "ReturnValue", double_str, "InInt")
link(double_str, "ReturnValue", double_print, "InString")
write("flow.txt", [ping, rearm, once, once_print, flip, ff, is_a, a_print, b_print, enter, open_, toggle, gate, through,
                   sum3, add3, sum_str, sum_print, hello, greet_call, use_double, double_call, double_str, double_print])

# Function graphs: node names repeat across graphs, as they do in real copies.
entry = N(G + "K2Node_FunctionEntry", "K2Node_FunctionEntry_0", ['FunctionReference=(MemberName="Greet")'],
          [("then", "out", EXEC), ("Who", "out", STR)])
hi = call("K2Node_CallFunction_0", STRL, "Concat_StrStr", [("A", "in", STR, "hi "), ("B", "in", STR, ""), ("ReturnValue", "out", STR)])
hi_print = print_("K2Node_CallFunction_1")
result = N(G + "K2Node_FunctionResult", "K2Node_FunctionResult_0", ['FunctionReference=(MemberName="Greet")'], [("execute", "in", EXEC)])
link(entry, "then", hi_print, "execute"); link(entry, "Who", hi, "B"); link(hi, "ReturnValue", hi_print, "InString")
link(hi_print, "then", result, "execute")
write("greet.txt", [entry, hi, hi_print, result])

entry = N(G + "K2Node_FunctionEntry", "K2Node_FunctionEntry_0", ['FunctionReference=(MemberName="Double")'],
          [("then", "out", EXEC), ("X", "out", INT)])
twice = math("K2Node_CallFunction_0", "Multiply_IntInt", INT, b="2")
result = N(G + "K2Node_FunctionResult", "K2Node_FunctionResult_0", ['FunctionReference=(MemberName="Double")'],
           [("execute", "in", EXEC), ("ReturnValue", "in", INT, "0")])
link(entry, "then", result, "execute"); link(entry, "X", twice, "A"); link(twice, "ReturnValue", result, "ReturnValue")
write("double.txt", [entry, twice, result])

# --- math.txt: trig, rounding, lerp, clamp and 2D vectors.
V2 = "struct"
def num_print(tag, src, src_pin, kind="int"):
    """Round (for floats) then print an int; returns the nodes and the print."""
    nodes = []
    if kind == "float":
        r = call(f"K2Node_CallFunction_r{tag}", MATH, "Round", [("A", "in", REAL, "0"), ("ReturnValue", "out", INT)])
        link(src, src_pin, r, "A"); nodes.append(r); src, src_pin = r, "ReturnValue"
    s_ = call(f"K2Node_CallFunction_s{tag}", MATH, "Conv_IntToString", [("InInt", "in", INT, "0"), ("ReturnValue", "out", STR)])
    pr = print_(f"K2Node_CallFunction_p{tag}")
    link(src, src_pin, s_, "InInt"); link(s_, "ReturnValue", pr, "InString")
    return nodes + [s_, pr], pr

def chain(start, start_pin, prints):
    prev, pp = start, start_pin
    for pr in prints:
        link(prev, pp, pr, "execute"); prev, pp = pr, "then"

m = []
trig = custom("K2Node_CustomEvent_0", "Trig", [("Deg", REAL)])
dsin = call("K2Node_CallFunction_0", MATH, "DegSin", [("A", "in", REAL, "0"), ("ReturnValue", "out", REAL)])
k1000 = call("K2Node_CallFunction_1", MATH, "Multiply_DoubleDouble", [("A", "in", REAL, "0"), ("B", "in", REAL, "1000.000000"), ("ReturnValue", "out", REAL)])
link(trig, "Deg", dsin, "A"); link(dsin, "ReturnValue", k1000, "A")
n1, p1 = num_print("t", k1000, "ReturnValue", "float")
chain(trig, "then", [p1]); m += [trig, dsin, k1000, *n1]

rnd = custom("K2Node_CustomEvent_1", "Rounding", [("X", REAL)])
fl = call("K2Node_CallFunction_2", MATH, "Floor", [("A", "in", REAL, "0"), ("ReturnValue", "out", INT)])
ro = call("K2Node_CallFunction_3", MATH, "Round", [("A", "in", REAL, "0"), ("ReturnValue", "out", INT)])
ce = call("K2Node_CallFunction_4", MATH, "Ceil", [("A", "in", REAL, "0"), ("ReturnValue", "out", INT)])
for f in (fl, ro, ce):
    link(rnd, "X", f, "A")
na, pa = num_print("f", fl, "ReturnValue"); nb, pb = num_print("o", ro, "ReturnValue"); nc, pc = num_print("c", ce, "ReturnValue")
chain(rnd, "then", [pa, pb, pc]); m += [rnd, fl, ro, ce, *na, *nb, *nc]

lc = custom("K2Node_CustomEvent_2", "LerpClamp", [("A", REAL), ("B", REAL), ("T", REAL), ("V", INT), ("Lo", INT), ("Hi", INT)])
lerp = call("K2Node_CallFunction_5", MATH, "Lerp", [("A", "in", REAL, "0"), ("B", "in", REAL, "0"), ("Alpha", "in", REAL, "0"), ("ReturnValue", "out", REAL)])
clamp = call("K2Node_CallFunction_6", MATH, "Clamp", [("Value", "in", INT, "0"), ("Min", "in", INT, "0"), ("Max", "in", INT, "0"), ("ReturnValue", "out", INT)])
link(lc, "A", lerp, "A"); link(lc, "B", lerp, "B"); link(lc, "T", lerp, "Alpha")
link(lc, "V", clamp, "Value"); link(lc, "Lo", clamp, "Min"); link(lc, "Hi", clamp, "Max")
nl, pl = num_print("l", lerp, "ReturnValue", "float"); ncl, pcl = num_print("k", clamp, "ReturnValue")
chain(lc, "then", [pl, pcl]); m += [lc, lerp, clamp, *nl, *ncl]

vec = custom("K2Node_CustomEvent_3", "Vec", [("X", REAL), ("Y", REAL)])
mk = call("K2Node_CallFunction_7", MATH, "MakeVector2D", [("X", "in", REAL, "0"), ("Y", "in", REAL, "0"), ("ReturnValue", "out", V2)])
nm = call("K2Node_CallFunction_8", MATH, "Normal2D", [("A", "in", V2, "(X=0.000000,Y=0.000000)"), ("Tolerance", "in", REAL, "0.000100"), ("ReturnValue", "out", V2)])
ln = call("K2Node_CallFunction_9", MATH, "VSize2D", [("A", "in", V2, "(X=0.000000,Y=0.000000)"), ("ReturnValue", "out", REAL)])
dt = call("K2Node_CallFunction_10", MATH, "DotProduct2D", [("A", "in", V2, "(X=0.000000,Y=0.000000)"), ("B", "in", V2, "(X=1.000000,Y=0.000000)"), ("ReturnValue", "out", REAL)])
br = call("K2Node_CallFunction_11", MATH, "BreakVector2D", [("InVec", "in", V2, "(X=0.000000,Y=0.000000)"), ("X", "out", REAL), ("Y", "out", REAL)])
h100 = call("K2Node_CallFunction_12", MATH, "Multiply_DoubleDouble", [("A", "in", REAL, "0"), ("B", "in", REAL, "100.000000"), ("ReturnValue", "out", REAL)])
sc = call("K2Node_CallFunction_13", MATH, "Multiply_Vector2DFloat", [("A", "in", V2, "(X=0.000000,Y=0.000000)"), ("B", "in", REAL, "2.000000"), ("ReturnValue", "out", V2)])
ad = call("K2Node_CallFunction_14", MATH, "Add_Vector2DVector2D", [("A", "in", V2, "(X=0.000000,Y=0.000000)"), ("B", "in", V2, "(X=0.000000,Y=0.000000)"), ("ReturnValue", "out", V2)])
br2 = call("K2Node_CallFunction_15", MATH, "BreakVector2D", [("InVec", "in", V2, "(X=0.000000,Y=0.000000)"), ("X", "out", REAL), ("Y", "out", REAL)])
link(vec, "X", mk, "X"); link(vec, "Y", mk, "Y")
for t, pn in ((nm, "A"), (ln, "A"), (dt, "A"), (sc, "A"), (ad, "A")):
    link(mk, "ReturnValue", t, pn)
link(nm, "ReturnValue", br, "InVec"); link(br, "X", h100, "A")
link(sc, "ReturnValue", ad, "B"); link(ad, "ReturnValue", br2, "InVec")
nv1, pv1 = num_print("v1", ln, "ReturnValue", "float")
nv2, pv2 = num_print("v2", dt, "ReturnValue", "float")
nv3, pv3 = num_print("v3", h100, "ReturnValue", "float")
nv4, pv4 = num_print("v4", br2, "Y", "float")
chain(vec, "then", [pv1, pv2, pv3, pv4])
m += [vec, mk, nm, ln, dt, br, h100, sc, ad, br2, *nv1, *nv2, *nv3, *nv4]
rem = custom("K2Node_CustomEvent_4", "Remainder", [("A", INT), ("B", INT)])
pc_ = call("K2Node_CallFunction_16", MATH, "Percent_IntInt", [("A", "in", INT, "0"), ("B", "in", INT, "0"), ("ReturnValue", "out", INT)])
link(rem, "A", pc_, "A"); link(rem, "B", pc_, "B")
nr_, pr_ = num_print("rm", pc_, "ReturnValue")
chain(rem, "then", [pr_]); m += [rem, pc_, *nr_]
sqe = custom("K2Node_CustomEvent_5", "Squared", [("X", REAL)])
sq_ = call("K2Node_CallFunction_17", MATH, "Square", [("A", "in", REAL, "0"), ("ReturnValue", "out", REAL)])
link(sqe, "X", sq_, "A")
nsq, psq = num_print("sq", sq_, "ReturnValue", "float")
chain(sqe, "then", [psq]); m += [sqe, sq_, *nsq]
lg = custom("K2Node_CustomEvent_6", "Logic", [("A", BOOL), ("B", BOOL)])
lg_prints = []
for i, mem in enumerate(("BooleanXOR", "BooleanNAND", "BooleanNOR")):
    c = call(f"K2Node_CallFunction_l{i}", MATH, mem, [("A", "in", BOOL, "false"), ("B", "in", BOOL, "false"), ("ReturnValue", "out", BOOL)])
    t = call(f"K2Node_CallFunction_lt{i}", STRL, "Conv_BoolToString", [("InBool", "in", BOOL, "false"), ("ReturnValue", "out", STR)])
    pr = print_(f"K2Node_CallFunction_lp{i}")
    link(lg, "A", c, "A"); link(lg, "B", c, "B"); link(c, "ReturnValue", t, "InBool"); link(t, "ReturnValue", pr, "InString")
    m += [c, t, pr]; lg_prints.append(pr)
chain(lg, "then", lg_prints); m.append(lg)
mi = custom("K2Node_CustomEvent_7", "Misc", [("N", INT), ("X", REAL), ("B", BOOL)])
si_ = call("K2Node_CallFunction_m0", MATH, "SignOfInteger", [("A", "in", INT, "0"), ("ReturnValue", "out", INT)])
sf_ = call("K2Node_CallFunction_m1", MATH, "SignOfFloat", [("A", "in", REAL, "0"), ("ReturnValue", "out", REAL)])
tn_ = call("K2Node_CallFunction_m2", MATH, "Tan", [("A", "in", REAL, "0"), ("ReturnValue", "out", REAL)])
bi_ = call("K2Node_CallFunction_m3", MATH, "Conv_BoolToInt", [("InBool", "in", BOOL, "false"), ("ReturnValue", "out", INT)])
ib_ = call("K2Node_CallFunction_m4", MATH, "Conv_IntToBool", [("InInt", "in", INT, "0"), ("ReturnValue", "out", BOOL)])
ibs = call("K2Node_CallFunction_m5", STRL, "Conv_BoolToString", [("InBool", "in", BOOL, "false"), ("ReturnValue", "out", STR)])
ibp = print_("K2Node_CallFunction_m6")
link(mi, "N", si_, "A"); link(mi, "X", sf_, "A"); link(mi, "X", tn_, "A"); link(mi, "B", bi_, "InBool"); link(mi, "N", ib_, "InInt")
link(ib_, "ReturnValue", ibs, "InBool"); link(ibs, "ReturnValue", ibp, "InString")
n0, p0 = num_print("m0", si_, "ReturnValue")
n1, p1 = num_print("m1", sf_, "ReturnValue", "float")
n2, p2 = num_print("m2", tn_, "ReturnValue", "float")
n3, p3 = num_print("m3", bi_, "ReturnValue")
chain(mi, "then", [p0, p1, p2, p3, ibp]); m += [mi, si_, sf_, tn_, bi_, ib_, ibs, ibp, *n0, *n1, *n2, *n3]
ne_ev = custom("K2Node_CustomEvent_8", "Near", [("A", REAL), ("B", REAL), ("N", INT)])
ne_ = call("K2Node_CallFunction_n0", MATH, "NearlyEqual_FloatFloat", [("A", "in", REAL, "0"), ("B", "in", REAL, "0"), ("ErrorTolerance", "in", REAL, "0.000100"), ("ReturnValue", "out", BOOL)])
nes = call("K2Node_CallFunction_n1", STRL, "Conv_BoolToString", [("InBool", "in", BOOL, "false"), ("ReturnValue", "out", STR)])
nep = print_("K2Node_CallFunction_n2")
cb_ = call("K2Node_CallFunction_n3", MATH, "Conv_ByteToInt", [("InByte", "in", INT, "0"), ("ReturnValue", "out", INT)])
link(ne_ev, "A", ne_, "A"); link(ne_ev, "B", ne_, "B"); link(ne_, "ReturnValue", nes, "InBool"); link(nes, "ReturnValue", nep, "InString")
link(ne_ev, "N", cb_, "InByte")
nn, pn = num_print("n4", cb_, "ReturnValue")
chain(ne_ev, "then", [nep, pn]); m += [ne_ev, ne_, nes, nep, cb_, *nn]
hy_ev = custom("K2Node_CustomEvent_9", "Hyp", [("X", REAL), ("Y", REAL), ("N", INT)])
hy_ = call("K2Node_CallFunction_h0", MATH, "Hypotenuse", [("Width", "in", REAL, "0"), ("Height", "in", REAL, "0"), ("ReturnValue", "out", REAL)])
fd_ = call("K2Node_CallFunction_h1", MATH, "Conv_FloatToDouble", [("InFloat", "in", REAL, "0"), ("ReturnValue", "out", REAL)])
bf_ = call("K2Node_CallFunction_h2", MATH, "Conv_ByteToFloat", [("InByte", "in", INT, "0"), ("ReturnValue", "out", REAL)])
ib2 = call("K2Node_CallFunction_h3", MATH, "Conv_IntToByte", [("InInt", "in", INT, "0"), ("ReturnValue", "out", INT)])
link(hy_ev, "X", hy_, "Width"); link(hy_ev, "Y", hy_, "Height"); link(hy_ev, "X", fd_, "InFloat"); link(hy_ev, "N", bf_, "InByte"); link(hy_ev, "N", ib2, "InInt")
h0n, h0p = num_print("h0", hy_, "ReturnValue", "float")
h1n, h1p = num_print("h1", fd_, "ReturnValue", "float")
h2n, h2p = num_print("h2", bf_, "ReturnValue", "float")
h3n, h3p = num_print("h3", ib2, "ReturnValue")
chain(hy_ev, "then", [h0p, h1p, h2p, h3p]); m += [hy_ev, hy_, fd_, bf_, ib2, *h0n, *h1n, *h2n, *h3n]
ss_ev = custom("K2Node_CustomEvent_10", "Search", [("S", STR), ("T", STR)])
ss_calls = []
ss_prints = []
for i, (mem, a_, b_, extra) in enumerate((("Contains", "SearchIn", "Substring", [("bUseCase", "in", BOOL, "false"), ("bSearchFromEnd", "in", BOOL, "false")]),
                                         ("StartsWith", "SourceString", "InPrefix", [("SearchCase", "in", "byte", "IgnoreCase")]),
                                         ("EndsWith", "SourceString", "InSuffix", [("SearchCase", "in", "byte", "CaseSensitive")]))):
    c = call(f"K2Node_CallFunction_q{i}", STRL, mem, [(a_, "in", STR, ""), (b_, "in", STR, ""), *extra, ("ReturnValue", "out", BOOL)])
    t = call(f"K2Node_CallFunction_qt{i}", STRL, "Conv_BoolToString", [("InBool", "in", BOOL, "false"), ("ReturnValue", "out", STR)])
    pr = print_(f"K2Node_CallFunction_qp{i}")
    link(ss_ev, "S", c, a_); link(ss_ev, "T", c, b_); link(c, "ReturnValue", t, "InBool"); link(t, "ReturnValue", pr, "InString")
    m += [c, t, pr]; ss_prints.append(pr)
chain(ss_ev, "then", ss_prints); m.append(ss_ev)
pw_ev = custom("K2Node_CustomEvent_11", "Pow", [("X", REAL), ("Y", REAL)])
pw_prints = []
for i, (mem, pins) in enumerate((("MultiplyMultiply_FloatFloat", (("Base", "X"), ("Exp", "Y"))), ("Exp", (("A", "X"),)), ("Loge", (("A", "X"),)),
                                 ("Log", (("A", "X"), ("Base", "Y"))), ("Atan2", (("Y", "Y"), ("X", "X"))), ("DegAtan2", (("Y", "Y"), ("X", "X"))),
                                 ("Asin", (("A", "Y"),)), ("DegAcos", (("A", "Y"),)), ("DegAtan", (("A", "X"),)))):
    c = call(f"K2Node_CallFunction_w{i}", MATH, mem, [*[(pn, "in", REAL, "0") for pn, _ in pins], ("ReturnValue", "out", REAL)])
    for pn, src in pins:
        link(pw_ev, src, c, pn)
    nn_, pp_ = num_print(f"w{i}", c, "ReturnValue", "float")
    m += [c, *nn_]; pw_prints.append(pp_)
chain(pw_ev, "then", pw_prints); m.append(pw_ev)
write("math.txt", m)

# --- loops.txt: a for loop with Break and a while loop.
lp = []
ff_ev = custom("K2Node_CustomEvent_0", "FindFirst", [("N", INT)])
fwb = macro("K2Node_MacroInstance_0", "ForLoopWithBreak", [("execute", "in", EXEC), ("FirstIndex", "in", INT, "0"), ("LastIndex", "in", INT, "9"),
            ("Break", "in", EXEC), ("LoopBody", "out", EXEC), ("Index", "out", INT), ("Completed", "out", EXEC)])
sq = math("K2Node_CallFunction_0", "Multiply_IntInt", INT)
ge_ = math("K2Node_CallFunction_1", "GreaterEqual_IntInt", INT, out=BOOL)
hit = N(G + "K2Node_IfThenElse", "K2Node_IfThenElse_0", [], [("execute", "in", EXEC), ("Condition", "in", BOOL, "true"), ("then", "out", EXEC), ("else", "out", EXEC)])
ni, pi = num_print("i", fwb, "Index")
done_print = print_("K2Node_CallFunction_2", "done")
link(ff_ev, "then", fwb, "execute"); link(fwb, "Index", sq, "A"); link(fwb, "Index", sq, "B")
link(sq, "ReturnValue", ge_, "A"); link(ff_ev, "N", ge_, "B"); link(ge_, "ReturnValue", hit, "Condition")
link(fwb, "LoopBody", hit, "execute"); link(hit, "then", pi, "execute"); link(pi, "then", fwb, "Break")
link(fwb, "Completed", done_print, "execute")
lp += [ff_ev, fwb, sq, ge_, hit, *ni, done_print]

cd_ev = custom("K2Node_CustomEvent_1", "Countdown", [("N", INT)])
set_left = var_set("K2Node_VariableSet_0", "Left", INT, "0")
wl = macro("K2Node_MacroInstance_1", "WhileLoop", [("execute", "in", EXEC), ("Condition", "in", BOOL, "false"),
           ("LoopBody", "out", EXEC), ("Completed", "out", EXEC)])
get_l1 = var_get("K2Node_VariableGet_0", "Left", INT)
gt0 = math("K2Node_CallFunction_3", "Greater_IntInt", INT, out=BOOL)
get_l2 = var_get("K2Node_VariableGet_1", "Left", INT)
nl_, pl_ = num_print("l", get_l2, "Left")
get_l3 = var_get("K2Node_VariableGet_2", "Left", INT)
dec = math("K2Node_CallFunction_4", "Subtract_IntInt", INT, b="1")
set_left2 = var_set("K2Node_VariableSet_1", "Left", INT, "0")
lift = print_("K2Node_CallFunction_5", "liftoff")
link(cd_ev, "then", set_left, "execute"); link(cd_ev, "N", set_left, "Left"); link(set_left, "then", wl, "execute")
link(get_l1, "Left", gt0, "A"); link(gt0, "ReturnValue", wl, "Condition")
link(wl, "LoopBody", pl_, "execute"); link(pl_, "then", set_left2, "execute")
link(get_l3, "Left", dec, "A"); link(dec, "ReturnValue", set_left2, "Left")
link(wl, "Completed", lift, "execute")
lp += [cd_ev, set_left, wl, get_l1, gt0, get_l2, *nl_, get_l3, dec, set_left2, lift]
write("loops.txt", lp)

# --- switch.txt: Switch on Int, Switch on String and Select.
sw = []
pick = custom("K2Node_CustomEvent_0", "Pick", [("N", INT)])
swi = N(G + "K2Node_SwitchInteger", "K2Node_SwitchInteger_0", ["StartIndex=0"],
        [("execute", "in", EXEC), ("Selection", "in", INT, "0"), ("NotEqual_IntInt", "in", "object"),
         ("0", "out", EXEC), ("1", "out", EXEC), ("2", "out", EXEC), ("Default", "out", EXEC)])
link(pick, "then", swi, "execute"); link(pick, "N", swi, "Selection")
sw += [pick, swi]
for k, (pin, text) in enumerate((("0", "zero"), ("1", "one"), ("2", "two"), ("Default", "other"))):
    pr = print_(f"K2Node_CallFunction_{k}", text); link(swi, pin, pr, "execute"); sw.append(pr)

word = custom("K2Node_CustomEvent_1", "Word", [("S", STR)])
sws = N(G + "K2Node_SwitchString", "K2Node_SwitchString_0", ['PinNames(0)="a"', 'PinNames(1)="b"', "bIsCaseSensitive=True"],
        [("execute", "in", EXEC), ("Selection", "in", STR, ""), ("a", "out", EXEC), ("b", "out", EXEC), ("Default", "out", EXEC)])
link(word, "then", sws, "execute"); link(word, "S", sws, "Selection")
sw += [word, sws]
for k, (pin, text) in enumerate((("a", "A"), ("b", "B"), ("Default", "?"))):
    pr = print_(f"K2Node_CallFunction_w{k}", text); link(sws, pin, pr, "execute"); sw.append(pr)

choose = custom("K2Node_CustomEvent_2", "Choose", [("I", INT), ("F", BOOL)])
seli = N(G + "K2Node_Select", "K2Node_Select_0", [],
         [("Option 0", "in", STR, "x"), ("Option 1", "in", STR, "y"), ("Option 2", "in", STR, "z"), ("Index", "in", INT, "0"), ("ReturnValue", "out", STR)])
selb = N(G + "K2Node_Select", "K2Node_Select_1", [],
         [("Option 0", "in", STR, "no"), ("Option 1", "in", STR, "yes"), ("Index", "in", BOOL, "false"), ("ReturnValue", "out", STR)])
p1 = print_("K2Node_CallFunction_c0"); p2 = print_("K2Node_CallFunction_c1")
link(choose, "I", seli, "Index"); link(choose, "F", selb, "Index")
link(seli, "ReturnValue", p1, "InString"); link(selb, "ReturnValue", p2, "InString")
chain(choose, "then", [p1, p2])
sw += [choose, seli, selb, p1, p2]
write("switch.txt", sw)

# --- timers.txt: set a timer by function name (once or looping), clear it.
def set_timer(name, fname, secs="0.000000", loop="false", delay="0.000000"):
    return call(name, SYS, "K2_SetTimer", [("Object", "in", "object"), ("FunctionName", "in", "name", fname),
                ("Time", "in", REAL, secs), ("bLooping", "in", BOOL, loop), ("InitialStartDelay", "in", REAL, delay),
                ("InitialStartDelayVariance", "in", REAL, "0.000000"), ("ReturnValue", "out", "struct")], pure=False)

def clear_timer(name, fname):
    return call(name, SYS, "K2_ClearTimer", [("Object", "in", "object"), ("FunctionName", "in", "name", fname)], pure=False)

start = custom("K2Node_CustomEvent_0", "Start", [("Secs", REAL), ("Loop", BOOL)])
st = set_timer("K2Node_CallFunction_0", "Tick")
link(start, "then", st, "execute"); link(start, "Secs", st, "Time"); link(start, "Loop", st, "bLooping")
stop = custom("K2Node_CustomEvent_1", "Stop")
ct = clear_timer("K2Node_CallFunction_1", "Tick")
link(stop, "then", ct, "execute")
tick = custom("K2Node_CustomEvent_2", "Tick")
tp = print_("K2Node_CallFunction_2", "tick")
link(tick, "then", tp, "execute")
write("timers.txt", [start, st, stop, ct, tick, tp])

# --- timers_bad.txt: timers Seal cannot import yet.
takes = custom("K2Node_CustomEvent_0", "Takes", [("X", INT)])
go = custom("K2Node_CustomEvent_1", "Go")
b1 = set_timer("K2Node_CallFunction_0", "Takes", "1.000000")
b2 = set_timer("K2Node_CallFunction_1", "Go", "1.000000", delay="0.500000")
b3 = set_timer("K2Node_CallFunction_2", "Nowhere", "1.000000")
chain(go, "then", [b1, b2, b3])
write("timers_bad.txt", [takes, go, b1, b2, b3])

# --- spawn.txt: spawn from a class, keep the actor, destroy it, check it.
def spawn(name, cls, extra=(), at=None):
    return N(G + "K2Node_SpawnActorFromClass", name, [],
             [("execute", "in", EXEC), ("then", "out", EXEC), ("WorldContextObject", "in", "object"),
              ("Class", "in", "class", "obj:/Game/Things/" + cls + "." + cls + "_C"), ("SpawnTransform", "in", "struct") if at is None else ("SpawnTransform", "in", "transform", at),
              ("CollisionHandlingOverride", "in", "byte", "Undefined"), ("Owner", "in", "object"), *extra,
              ("ReturnValue", "out", "object")])

def destroy(name):
    return N(G + "K2Node_CallFunction", name, ['FunctionReference=(MemberName="K2_DestroyActor",bSelfContext=True)'],
             [("execute", "in", EXEC), ("then", "out", EXEC), ("self", "in", "object")])

sp = []
make = custom("K2Node_CustomEvent_0", "Make")
s1 = spawn("K2Node_SpawnActorFromClass_0", "BP_Coin")
keep = var_set("K2Node_VariableSet_0", "Last", "object")
made = print_("K2Node_CallFunction_0", "made")
link(make, "then", s1, "execute"); link(s1, "ReturnValue", keep, "Last")
chain(s1, "then", [keep, made])
sp += [make, s1, keep, made]

kill = custom("K2Node_CustomEvent_1", "Kill")
last1 = var_get("K2Node_VariableGet_0", "Last", "object")
d1 = destroy("K2Node_CallFunction_1")
link(kill, "then", d1, "execute"); link(last1, "Last", d1, "self")
sp += [kill, last1, d1]

check = custom("K2Node_CustomEvent_2", "Check")
last2 = var_get("K2Node_VariableGet_1", "Last", "object")
iv = macro("K2Node_MacroInstance_0", "IsValid", [("exec", "in", EXEC), ("InputObject", "in", "object"), ("Is Valid", "out", EXEC), ("Is Not Valid", "out", EXEC)])
yes = print_("K2Node_CallFunction_2", "valid"); no = print_("K2Node_CallFunction_3", "gone")
link(check, "then", iv, "exec"); link(last2, "Last", iv, "InputObject")
link(iv, "Is Valid", yes, "execute"); link(iv, "Is Not Valid", no, "execute")
sp += [check, last2, iv, yes, no]

ask = custom("K2Node_CustomEvent_3", "Ask")
last3 = var_get("K2Node_VariableGet_2", "Last", "object")
piv = call("K2Node_CallFunction_4", SYS, "IsValid", [("Object", "in", "object"), ("ReturnValue", "out", BOOL)])
br = N(G + "K2Node_IfThenElse", "K2Node_IfThenElse_0", [], [("execute", "in", EXEC), ("Condition", "in", BOOL, "true"), ("then", "out", EXEC), ("else", "out", EXEC)])
ay = print_("K2Node_CallFunction_5", "yes"); an = print_("K2Node_CallFunction_6", "no")
link(ask, "then", br, "execute"); link(last3, "Last", piv, "Object"); link(piv, "ReturnValue", br, "Condition")
link(br, "then", ay, "execute"); link(br, "else", an, "execute")
sp += [ask, last3, piv, br, ay, an]

bye = custom("K2Node_CustomEvent_4", "Bye")
d2 = destroy("K2Node_CallFunction_7")
link(bye, "then", d2, "execute")
sp += [bye, d2]
write("spawn.txt", sp)

# --- spawn_bad.txt: a value set on spawn, which Seal cannot carry yet.
go2 = custom("K2Node_CustomEvent_0", "Go")
s2 = spawn("K2Node_SpawnActorFromClass_0", "BP_Coin", [("Speed", "in", REAL, "3.000000")])
link(go2, "then", s2, "execute")
write("spawn_bad.txt", [go2, s2])

# --- casts.txt: Cast To on a kept actor, branching and pure.
def cast(name, cls, pure=False):
    pins = [] if pure else [("execute", "in", EXEC), ("then", "out", EXEC), ("CastFailed", "out", EXEC)]
    pins += [("Object", "in", "object"), ("As" + cls.replace("_", " "), "out", "object"), ("bSuccess", "out", BOOL)]
    props = [f"TargetType=\"/Script/Engine.BlueprintGeneratedClass'/Game/Things/{cls}.{cls}_C'\""]
    if pure:
        props.append("bIsPureCast=True")
    return N(G + "K2Node_DynamicCast", name, props, pins)

cs = []
for i, cls in enumerate(("BP_Boss", "BP_Coin")):
    ev = custom(f"K2Node_CustomEvent_{i}", "Make" + cls[3:])
    s_ = spawn(f"K2Node_SpawnActorFromClass_{i}", cls)
    keep = var_set(f"K2Node_VariableSet_{i}", "Last", "object")
    link(ev, "then", s_, "execute"); link(s_, "ReturnValue", keep, "Last"); link(s_, "then", keep, "execute")
    cs += [ev, s_, keep]
chk = custom("K2Node_CustomEvent_2", "Check")
lg = var_get("K2Node_VariableGet_0", "Last", "object")
c1 = cast("K2Node_DynamicCast_0", "BP_Enemy")
c2 = cast("K2Node_DynamicCast_1", "BP_Boss", pure=True)
ok = print_("K2Node_CallFunction_0", "enemy"); bad = print_("K2Node_CallFunction_1", "not an enemy")
bs = call("K2Node_CallFunction_2", STRL, "Conv_BoolToString", [("InBool", "in", BOOL, "false"), ("ReturnValue", "out", STR)])
bp = print_("K2Node_CallFunction_3")
link(chk, "then", c1, "execute"); link(lg, "Last", c1, "Object"); link(lg, "Last", c2, "Object")
link(c1, "then", ok, "execute"); link(c1, "CastFailed", bad, "execute")
link(c2, "bSuccess", bs, "InBool"); link(bs, "ReturnValue", bp, "InString")
link(ok, "then", bp, "execute"); link(bad, "then", bp, "execute")
kill = custom("K2Node_CustomEvent_3", "Kill")
lg2 = var_get("K2Node_VariableGet_1", "Last", "object")
c3 = cast("K2Node_DynamicCast_2", "BP_Boss", pure=True)
d_ = destroy("K2Node_CallFunction_4")
link(kill, "then", d_, "execute"); link(lg2, "Last", c3, "Object"); link(c3, "AsBP Boss", d_, "self")
cs += [chk, lg, c1, c2, ok, bad, bs, bp, kill, lg2, c3, d_]
write("casts.txt", cs)
nat = custom("K2Node_CustomEvent_0", "Go")
lg3 = var_get("K2Node_VariableGet_0", "Last", "object")
c4 = N(G + "K2Node_DynamicCast", "K2Node_DynamicCast_0", ["TargetType=\"/Script/CoreUObject.Class'/Script/Engine.Character'\""],
       [("execute", "in", EXEC), ("then", "out", EXEC), ("CastFailed", "out", EXEC), ("Object", "in", "object"), ("AsCharacter", "out", "object"), ("bSuccess", "out", BOOL)])
link(nat, "then", c4, "execute"); link(lg3, "Last", c4, "Object")
write("casts_bad.txt", [nat, lg3, c4])

# --- vec3.txt: 3D vector math and actor location.
V3, ZERO3 = "vector", "0.000000,0.000000,0.000000"
def brk(name):
    return call(name, MATH, "BreakVector", [("InVec", "in", V3, ZERO3), ("X", "out", REAL), ("Y", "out", REAL), ("Z", "out", REAL)])
v = []
ev = custom("K2Node_CustomEvent_0", "Vec3", [("X", REAL), ("Y", REAL), ("Z", REAL)])
mk = call("K2Node_CallFunction_0", MATH, "MakeVector", [("X", "in", REAL, "0"), ("Y", "in", REAL, "0"), ("Z", "in", REAL, "0"), ("ReturnValue", "out", V3)])
sz = call("K2Node_CallFunction_1", MATH, "VSize", [("A", "in", V3, ZERO3), ("ReturnValue", "out", REAL)])
cr = call("K2Node_CallFunction_2", MATH, "Cross_VectorVector", [("A", "in", V3, ZERO3), ("B", "in", V3, "0.000000,0.000000,1.000000"), ("ReturnValue", "out", V3)])
dt = call("K2Node_CallFunction_3", MATH, "Dot_VectorVector", [("A", "in", V3, ZERO3), ("B", "in", V3, "1.000000,2.000000,3.000000"), ("ReturnValue", "out", REAL)])
sc = call("K2Node_CallFunction_4", MATH, "Multiply_VectorFloat", [("A", "in", V3, ZERO3), ("B", "in", REAL, "2.000000"), ("ReturnValue", "out", V3)])
ad = call("K2Node_CallFunction_5", MATH, "Add_VectorVector", [("A", "in", V3, ZERO3), ("B", "in", V3, ZERO3), ("ReturnValue", "out", V3)])
ng = call("K2Node_CallFunction_6", MATH, "NegateVector", [("A", "in", V3, ZERO3), ("ReturnValue", "out", V3)])
nm = call("K2Node_CallFunction_7", MATH, "Normal", [("A", "in", V3, ZERO3), ("Tolerance", "in", REAL, "0.000100"), ("ReturnValue", "out", V3)])
bc, ba, bn, bm = brk("K2Node_CallFunction_8"), brk("K2Node_CallFunction_9"), brk("K2Node_CallFunction_10"), brk("K2Node_CallFunction_11")
k100 = call("K2Node_CallFunction_12", MATH, "Multiply_DoubleDouble", [("A", "in", REAL, "0"), ("B", "in", REAL, "100.000000"), ("ReturnValue", "out", REAL)])
link(ev, "X", mk, "X"); link(ev, "Y", mk, "Y"); link(ev, "Z", mk, "Z")
for t in (sz, cr, dt, sc, ng, nm):
    link(mk, "ReturnValue", t, "A")
link(mk, "ReturnValue", ad, "A"); link(sc, "ReturnValue", ad, "B")
link(cr, "ReturnValue", bc, "InVec"); link(ad, "ReturnValue", ba, "InVec"); link(ng, "ReturnValue", bn, "InVec"); link(nm, "ReturnValue", bm, "InVec")
link(bm, "X", k100, "A")
outs = [num_print("a", sz, "ReturnValue", "float"), num_print("b", bc, "Y", "float"), num_print("c", dt, "ReturnValue", "float"),
        num_print("d", ba, "Z", "float"), num_print("e", bn, "Y", "float"), num_print("f", k100, "ReturnValue", "float")]
chain(ev, "then", [p for _, p in outs])
v += [ev, mk, sz, cr, dt, sc, ad, ng, nm, bc, ba, bn, bm, k100, *[n for ns, _ in outs for n in ns]]

def actor(name, member, pins, pure=False):
    ex = [] if pure else [("execute", "in", EXEC), ("then", "out", EXEC)]
    props = [f'FunctionReference=(MemberName="{member}",bSelfContext=True)'] + (["bDefaultsToPureFunc=True"] if pure else [])
    return N(G + "K2Node_CallFunction", name, props, [*ex, ("self", "in", "object"), *pins])
MOVE = [("bSweep", "in", BOOL, "false"), ("SweepHitResult", "out", "struct"), ("bTeleport", "in", BOOL, "false"), ("ReturnValue", "out", BOOL)]
mv = custom("K2Node_CustomEvent_1", "Move")
setl = actor("K2Node_CallFunction_20", "K2_SetActorLocation", [("NewLocation", "in", V3, "10.000000,20.000000,30.000000"), *MOVE])
off = actor("K2Node_CallFunction_21", "K2_AddActorWorldOffset", [("DeltaLocation", "in", V3, "1.500000,0.000000,-4.000000"), *MOVE])
getl = actor("K2Node_CallFunction_22", "K2_GetActorLocation", [("ReturnValue", "out", V3)], pure=True)
bl = brk("K2Node_CallFunction_23")
link(getl, "ReturnValue", bl, "InVec")
o1, o2, o3 = num_print("x", bl, "X", "float"), num_print("y", bl, "Y", "float"), num_print("z", bl, "Z", "float")
chain(mv, "then", [setl, off, o1[1], o2[1], o3[1]])
v += [mv, setl, off, getl, bl, *o1[0], *o2[0], *o3[0]]
write("vec3.txt", v)

# --- vec3_bad.txt: a sweeping move, which Seal does not map.
go3 = custom("K2Node_CustomEvent_0", "Go")
sw = actor("K2Node_CallFunction_0", "K2_SetActorLocation", [("NewLocation", "in", V3, ZERO3), ("bSweep", "in", BOOL, "true"), *MOVE[1:]])
link(go3, "then", sw, "execute")
write("vec3_bad.txt", [go3, sw])

# --- spawn_at.txt: spawn at a literal transform, and at one made from a vector.
def make_tf(name, loc=ZERO3, rot=ZERO3, scale="1.000000,1.000000,1.000000"):
    return call(name, MATH, "MakeTransform", [("Location", "in", V3, loc), ("Rotation", "in", "rotator", rot),
                ("Scale", "in", V3, scale), ("ReturnValue", "out", "transform")])
sa = []
place = custom("K2Node_CustomEvent_0", "Place")
s3 = spawn("K2Node_SpawnActorFromClass_0", "BP_Coin", at="5.000000,6.000000,7.000000|0.000000,0.000000,0.000000|1.000000,1.000000,1.000000")
keep3 = var_set("K2Node_VariableSet_0", "Last", "object")
link(place, "then", s3, "execute"); link(s3, "then", keep3, "execute"); link(s3, "ReturnValue", keep3, "Last")
sa += [place, s3, keep3]
put = custom("K2Node_CustomEvent_1", "Put", [("X", REAL)])
mkv = call("K2Node_CallFunction_0", MATH, "MakeVector", [("X", "in", REAL, "0.0"), ("Y", "in", REAL, "2.500000"), ("Z", "in", REAL, "-1.000000"), ("ReturnValue", "out", V3)])
tf = make_tf("K2Node_CallFunction_1")
s4 = spawn("K2Node_SpawnActorFromClass_1", "BP_Coin")
keep4 = var_set("K2Node_VariableSet_1", "Last", "object")
link(put, "X", mkv, "X"); link(mkv, "ReturnValue", tf, "Location"); link(tf, "ReturnValue", s4, "SpawnTransform")
link(put, "then", s4, "execute"); link(s4, "then", keep4, "execute"); link(s4, "ReturnValue", keep4, "Last")
sa += [put, mkv, tf, s4, keep4]
tilt = custom("K2Node_CustomEvent_2", "Tilt")
s7 = spawn("K2Node_SpawnActorFromClass_2", "BP_Coin", at="1.000000,2.000000,3.000000|0.000000,90.000000,0.000000|2.000000,2.000000,2.000000")
keep7 = var_set("K2Node_VariableSet_2", "Last", "object")
link(tilt, "then", s7, "execute"); link(s7, "then", keep7, "execute"); link(s7, "ReturnValue", keep7, "Last")
sa += [tilt, s7, keep7]
write("spawn_at.txt", sa)

# --- spawn_at_bad.txt: a made transform with scale, which Seal does not carry yet.
go4 = custom("K2Node_CustomEvent_0", "Go")
tf2 = make_tf("K2Node_CallFunction_0", scale="2.000000,2.000000,2.000000")
s6 = spawn("K2Node_SpawnActorFromClass_1", "BP_Coin")
link(tf2, "ReturnValue", s6, "SpawnTransform")
link(go4, "then", s6, "execute")
write("spawn_at_bad.txt", [go4, tf2, s6])

# --- rot.txt: actor rotation and scale, and making and breaking rotators.
R = "rotator"
turn = custom("K2Node_CustomEvent_0", "Turn", [("Yaw", REAL)])
mkr = call("K2Node_CallFunction_0", MATH, "MakeRotator", [("Roll", "in", REAL, "-5.000000"), ("Pitch", "in", REAL, "10.000000"),
           ("Yaw", "in", REAL, "0.0"), ("ReturnValue", "out", R)])
setr = actor("K2Node_CallFunction_1", "K2_SetActorRotation", [("NewRotation", "in", R, ZERO3), ("bTeleportPhysics", "in", BOOL, "false"), ("ReturnValue", "out", BOOL)])
sets = actor("K2Node_CallFunction_2", "SetActorScale3D", [("NewScale3D", "in", V3, "2.000000,3.000000,4.000000")])
getr = actor("K2Node_CallFunction_3", "K2_GetActorRotation", [("ReturnValue", "out", R)], pure=True)
brr = call("K2Node_CallFunction_4", MATH, "BreakRotator", [("InRot", "in", R, ZERO3), ("Roll", "out", REAL), ("Pitch", "out", REAL), ("Yaw", "out", REAL)])
gets = actor("K2Node_CallFunction_5", "GetActorScale3D", [("ReturnValue", "out", V3)], pure=True)
bs = brk("K2Node_CallFunction_6")
link(turn, "Yaw", mkr, "Yaw"); link(mkr, "ReturnValue", setr, "NewRotation"); link(getr, "ReturnValue", brr, "InRot")
link(gets, "ReturnValue", bs, "InVec")
rn = [turn, mkr, setr, sets, getr, brr, gets, bs]
prs = []
for tag, (src, pin) in [("p", (brr, "Pitch")), ("y", (brr, "Yaw")), ("r", (brr, "Roll")), ("z", (bs, "Z"))]:
    ns, pr = num_print(tag, src, pin, "float"); rn += ns; prs.append(pr)
link(turn, "then", setr, "execute"); link(setr, "then", sets, "execute")
chain(sets, "then", prs)
write("rot.txt", rn)

# --- don.txt: the Do N macro with a counter output and a reset.
hit = custom("K2Node_CustomEvent_0", "Hit")
back = custom("K2Node_CustomEvent_1", "Rearm")
don = macro("K2Node_MacroInstance_0", "DoN", [("Enter", "in", EXEC), ("N", "in", INT, "3"), ("Reset", "in", EXEC), ("Exit", "out", EXEC), ("Counter", "out", INT)])
dstr = call("K2Node_CallFunction_0", MATH, "Conv_IntToString", [("InInt", "in", INT, "0"), ("ReturnValue", "out", STR)])
dpr = print_("K2Node_CallFunction_1")
link(hit, "then", don, "Enter"); link(back, "then", don, "Reset"); link(don, "Exit", dpr, "execute")
link(don, "Counter", dstr, "InInt"); link(dstr, "ReturnValue", dpr, "InString")
write("don.txt", [hit, back, don, dstr, dpr])

# --- arrays.txt: an array variable changed in place, read, looped over with
# For Each (with and without Break), built with Make Array, and cleared.
ARR = "MemberParent=/Script/CoreUObject.Class'\"/Script/Engine.KismetArrayLibrary\"'"
IA = "int[]"
def arr(name, member, pins, pure=False):
    return call(name, ARR, member, [("TargetArray", "in", IA), *pins], pure=pure)
def int_print(tag, src, pin):
    return num_print(tag, src, pin)
a = []
fill = custom("K2Node_CustomEvent_0", "Fill", [("X", INT)])
g0 = var_get("K2Node_VariableGet_0", "Scores", IA)
add = arr("K2Node_CallFunction_0", "Array_Add", [("NewItem", "in", INT, "0"), ("ReturnValue", "out", INT)])
g1 = var_get("K2Node_VariableGet_1", "Scores", IA)
ln = arr("K2Node_CallFunction_1", "Array_Length", [("ReturnValue", "out", INT)], pure=True)
link(g0, "Scores", add, "TargetArray"); link(fill, "X", add, "NewItem"); link(g1, "Scores", ln, "TargetArray")
n1, p1 = num_print("a", add, "ReturnValue"); n2, p2 = num_print("b", ln, "ReturnValue")
link(fill, "then", add, "execute"); chain(add, "then", [p1, p2])
a += [fill, g0, add, g1, ln, *n1, *n2]

sm = custom("K2Node_CustomEvent_1", "Sum")
zero = var_set("K2Node_VariableSet_0", "Total", INT, "0")
g2 = var_get("K2Node_VariableGet_2", "Scores", IA)
fe = macro("K2Node_MacroInstance_0", "ForEachLoop", [("Exec", "in", EXEC), ("Array", "in", IA), ("LoopBody", "out", EXEC),
           ("Array Element", "out", INT), ("Array Index", "out", INT), ("Completed", "out", EXEC)])
gt = var_get("K2Node_VariableGet_3", "Total", INT)
plus = math("K2Node_CallFunction_2", "Add_IntInt", INT)
st = var_set("K2Node_VariableSet_1", "Total", INT)
gt2 = var_get("K2Node_VariableGet_4", "Total", INT)
link(sm, "then", zero, "execute"); link(zero, "then", fe, "Exec"); link(g2, "Scores", fe, "Array")
link(fe, "LoopBody", st, "execute"); link(gt, "Total", plus, "A"); link(fe, "Array Element", plus, "B"); link(plus, "ReturnValue", st, "Total")
n3, p3 = num_print("c", gt2, "Total"); link(fe, "Completed", p3, "execute")
a += [sm, zero, g2, fe, gt, plus, st, gt2, *n3]

q = custom("K2Node_CustomEvent_2", "Query", [("X", INT)])
g3 = var_get("K2Node_VariableGet_5", "Scores", IA)
fnd = arr("K2Node_CallFunction_3", "Array_Find", [("ItemToFind", "in", INT, "0"), ("ReturnValue", "out", INT)], pure=True)
has = arr("K2Node_CallFunction_4", "Array_Contains", [("ItemToFind", "in", INT, "0"), ("ReturnValue", "out", BOOL)], pure=True)
hs = call("K2Node_CallFunction_5", STRL, "Conv_BoolToString", [("InBool", "in", BOOL, "false"), ("ReturnValue", "out", STR)])
hp = print_("K2Node_CallFunction_6")
item = N(G + "K2Node_GetArrayItem", "K2Node_GetArrayItem_0", [], [("Array", "in", IA), ("Dimension 1", "in", INT, "0"), ("Output", "out", INT)])
last = arr("K2Node_CallFunction_7", "Array_LastIndex", [("ReturnValue", "out", INT)], pure=True)
for n_ in (fnd, has, last):
    link(g3, "Scores", n_, "TargetArray")
link(g3, "Scores", item, "Array"); link(q, "X", fnd, "ItemToFind"); link(q, "X", has, "ItemToFind"); link(q, "X", item, "Dimension 1")
link(has, "ReturnValue", hs, "InBool"); link(hs, "ReturnValue", hp, "InString")
n4, p4 = num_print("d", fnd, "ReturnValue"); n5, p5 = num_print("e", item, "Output"); n6, p6 = num_print("f", last, "ReturnValue")
chain(q, "then", [p4, hp, p5, p6])
a += [q, g3, fnd, has, hs, hp, item, last, *n4, *n5, *n6]

ed = custom("K2Node_CustomEvent_3", "Edit", [("I", INT), ("X", INT)])
gs = [var_get(f"K2Node_VariableGet_{k}", "Scores", IA) for k in range(6, 10)]
ins = arr("K2Node_CallFunction_8", "Array_Insert", [("NewItem", "in", INT, "0"), ("Index", "in", INT, "0")])
rm = arr("K2Node_CallFunction_9", "Array_Remove", [("IndexToRemove", "in", INT, "0")])
setx = arr("K2Node_CallFunction_10", "Array_Set", [("Index", "in", INT, "2"), ("Item", "in", INT, "0"), ("bSizeToFit", "in", BOOL, "false")])
rmi = arr("K2Node_CallFunction_11", "Array_RemoveItem", [("Item", "in", INT, "7"), ("ReturnValue", "out", BOOL)])
for g_, n_ in zip(gs, (ins, rm, setx, rmi)):
    link(g_, "Scores", n_, "TargetArray")
link(ed, "X", ins, "NewItem"); link(ed, "I", ins, "Index"); link(ed, "X", setx, "Item")
link(ed, "then", ins, "execute"); link(ins, "then", rm, "execute"); link(rm, "then", setx, "execute"); link(setx, "then", rmi, "execute")
a += [ed, *gs, ins, rm, setx, rmi]

ff = custom("K2Node_CustomEvent_4", "FindFirst", [("X", INT)])
g4 = var_get("K2Node_VariableGet_10", "Scores", IA)
fb = macro("K2Node_MacroInstance_1", "ForEachLoopWithBreak", [("Exec", "in", EXEC), ("Array", "in", IA), ("Break", "in", EXEC), ("LoopBody", "out", EXEC),
           ("Array Element", "out", INT), ("Array Index", "out", INT), ("Completed", "out", EXEC)])
eq = math("K2Node_CallFunction_12", "EqualEqual_IntInt", INT, out=BOOL)
br = N(G + "K2Node_IfThenElse", "K2Node_IfThenElse_0", [], [("execute", "in", EXEC), ("Condition", "in", BOOL, "false"), ("then", "out", EXEC), ("else", "out", EXEC)])
n7, p7 = num_print("g", fb, "Array Index")
dn = print_("K2Node_CallFunction_13", "done")
link(ff, "then", fb, "Exec"); link(g4, "Scores", fb, "Array"); link(fb, "Array Element", eq, "A"); link(ff, "X", eq, "B")
link(fb, "LoopBody", br, "execute"); link(eq, "ReturnValue", br, "Condition"); link(br, "then", p7, "execute"); link(p7, "then", fb, "Break")
link(fb, "Completed", dn, "execute")
a += [ff, g4, fb, eq, br, *n7, dn]

rs = custom("K2Node_CustomEvent_5", "Reset", [("X", INT)])
mk = N(G + "K2Node_MakeArray", "K2Node_MakeArray_0", ["NumInputs=3"], [("Array", "out", IA), ("[0]", "in", INT, "7"), ("[1]", "in", INT, "0"), ("[2]", "in", INT, "9")])
ss = var_set("K2Node_VariableSet_2", "Scores", IA)
link(rs, "X", mk, "[1]"); link(mk, "Array", ss, "Scores"); link(rs, "then", ss, "execute")
a += [rs, mk, ss]

wp = custom("K2Node_CustomEvent_6", "Wipe")
g5 = var_get("K2Node_VariableGet_11", "Scores", IA)
clr = arr("K2Node_CallFunction_14", "Array_Clear", [])
g6 = var_get("K2Node_VariableGet_12", "Scores", IA)
ln2 = arr("K2Node_CallFunction_15", "Array_Length", [("ReturnValue", "out", INT)], pure=True)
link(g5, "Scores", clr, "TargetArray"); link(g6, "Scores", ln2, "TargetArray")
n8, p8 = num_print("h", ln2, "ReturnValue")
link(wp, "then", clr, "execute"); link(clr, "then", p8, "execute")
a += [wp, g5, clr, g6, ln2, *n8]
write("arrays.txt", a)

# --- finds.txt: Get All Actors Of Class, counted and looped over to destroy.
GS = "MemberParent=/Script/CoreUObject.Class'\"/Script/Engine.GameplayStatics\"'"
OA = "object[]"
def get_all(name, cls):
    return call(name, GS, "GetAllActorsOfClass", [("WorldContextObject", "in", "object"),
                ("ActorClass", "in", "class", "obj:" + cls), ("OutActors", "out", OA)], pure=False)
fs = []
for i, cls in enumerate(("BP_Boss", "BP_Coin")):
    ev = custom(f"K2Node_CustomEvent_{i}", "Make" + cls[3:])
    s_ = spawn(f"K2Node_SpawnActorFromClass_{i}", cls)
    link(ev, "then", s_, "execute")
    fs += [ev, s_]
cnt = custom("K2Node_CustomEvent_2", "Count")
ga = get_all("K2Node_CallFunction_0", "/Game/Things/BP_Enemy.BP_Enemy_C")
ln = arr("K2Node_CallFunction_1", "Array_Length", [("ReturnValue", "out", INT)], pure=True)
next(p for p in ln.pins if p[0] == "TargetArray")[2] = OA
link(ga, "OutActors", ln, "TargetArray")
n1, p1 = num_print("a", ln, "ReturnValue")
link(cnt, "then", ga, "execute"); link(ga, "then", p1, "execute")
fs += [cnt, ga, ln, *n1]
pg = custom("K2Node_CustomEvent_3", "Purge")
ga2 = get_all("K2Node_CallFunction_2", "/Game/Things/BP_Enemy.BP_Enemy_C")
fe = macro("K2Node_MacroInstance_0", "ForEachLoop", [("Exec", "in", EXEC), ("Array", "in", OA), ("LoopBody", "out", EXEC),
           ("Array Element", "out", "object"), ("Array Index", "out", INT), ("Completed", "out", EXEC)])
d_ = destroy("K2Node_CallFunction_3")
link(pg, "then", ga2, "execute"); link(ga2, "then", fe, "Exec"); link(ga2, "OutActors", fe, "Array")
link(fe, "LoopBody", d_, "execute"); link(fe, "Array Element", d_, "self")
fs += [pg, ga2, fe, d_]
one = custom("K2Node_CustomEvent_4", "KillOne")
g1 = call("K2Node_CallFunction_4", GS, "GetActorOfClass", [("WorldContextObject", "in", "object"),
          ("ActorClass", "in", "class", "obj:/Game/Things/BP_Enemy.BP_Enemy_C"), ("ReturnValue", "out", "object")], pure=False)
d2 = destroy("K2Node_CallFunction_5")
link(one, "then", g1, "execute"); link(g1, "then", d2, "execute"); link(g1, "ReturnValue", d2, "self")
fs += [one, g1, d2]
write("finds.txt", fs)
nat = custom("K2Node_CustomEvent_0", "Go")
ga3 = get_all("K2Node_CallFunction_0", "/Script/Engine.Character")
link(nat, "then", ga3, "execute")
write("finds_bad.txt", [nat, ga3])

# --- equal.txt: string, name and object equality, and text/name conversions.
eqs = []
same = custom("K2Node_CustomEvent_0", "Same", [("A", STR), ("B", STR)])
st = call("K2Node_CallFunction_0", STRL, "Conv_StringToText", [("InString", "in", STR, ""), ("ReturnValue", "out", "text")])
ts = call("K2Node_CallFunction_1", "MemberParent=/Script/CoreUObject.Class'\"/Script/Engine.KismetTextLibrary\"'", "Conv_TextToString",
          [("InText", "in", "text", ""), ("ReturnValue", "out", STR)])
e1 = call("K2Node_CallFunction_2", STRL, "EqualEqual_StrStr", [("A", "in", STR, ""), ("B", "in", STR, ""), ("ReturnValue", "out", BOOL)])
n1 = call("K2Node_CallFunction_3", STRL, "Conv_StringToName", [("InString", "in", STR, ""), ("ReturnValue", "out", "name")])
n2 = call("K2Node_CallFunction_4", STRL, "Conv_StringToName", [("InString", "in", STR, ""), ("ReturnValue", "out", "name")])
e2 = call("K2Node_CallFunction_5", MATH, "NotEqual_NameName", [("A", "in", "name", "None"), ("B", "in", "name", "None"), ("ReturnValue", "out", BOOL)])
b1 = call("K2Node_CallFunction_6", STRL, "Conv_BoolToString", [("InBool", "in", BOOL, "false"), ("ReturnValue", "out", STR)])
b2 = call("K2Node_CallFunction_7", STRL, "Conv_BoolToString", [("InBool", "in", BOOL, "false"), ("ReturnValue", "out", STR)])
p1 = print_("K2Node_CallFunction_8"); p2 = print_("K2Node_CallFunction_9")
link(same, "A", st, "InString"); link(st, "ReturnValue", ts, "InText"); link(ts, "ReturnValue", e1, "A"); link(same, "B", e1, "B")
link(same, "A", n1, "InString"); link(same, "B", n2, "InString"); link(n1, "ReturnValue", e2, "A"); link(n2, "ReturnValue", e2, "B")
link(e1, "ReturnValue", b1, "InBool"); link(e2, "ReturnValue", b2, "InBool"); link(b1, "ReturnValue", p1, "InString"); link(b2, "ReturnValue", p2, "InString")
link(same, "then", p1, "execute"); link(p1, "then", p2, "execute")
eqs += [same, st, ts, e1, n1, n2, e2, b1, b2, p1, p2]
mk = custom("K2Node_CustomEvent_1", "Make")
sp_ = spawn("K2Node_SpawnActorFromClass_0", "BP_Coin")
lg = var_get("K2Node_VariableGet_0", "Last", "object")
oe = call("K2Node_CallFunction_10", MATH, "EqualEqual_ObjectObject", [("A", "in", "object"), ("B", "in", "object"), ("ReturnValue", "out", BOOL)])
ob = call("K2Node_CallFunction_11", STRL, "Conv_BoolToString", [("InBool", "in", BOOL, "false"), ("ReturnValue", "out", STR)])
op_ = print_("K2Node_CallFunction_12")
keep = var_set("K2Node_VariableSet_0", "Last", "object")
link(mk, "then", sp_, "execute"); link(sp_, "then", op_, "execute"); link(op_, "then", keep, "execute")
link(lg, "Last", oe, "A"); link(sp_, "ReturnValue", oe, "B"); link(oe, "ReturnValue", ob, "InBool"); link(ob, "ReturnValue", op_, "InString")
link(sp_, "ReturnValue", keep, "Last")
eqs += [mk, sp_, lg, oe, ob, op_, keep]
write("equal.txt", eqs)

# --- time.txt: frame length and game time, read in Tick and in an event.
tm = []
tick = event("K2Node_Event_0", "ReceiveTick", [("DeltaSeconds", "out", REAL)])
wd = call("K2Node_CallFunction_0", GS, "GetWorldDeltaSeconds", [("WorldContextObject", "in", "object"), ("ReturnValue", "out", REAL)])
sd = var_set("K2Node_VariableSet_0", "Frame", REAL)
link(tick, "then", sd, "execute"); link(wd, "ReturnValue", sd, "Frame")
tm += [tick, wd, sd]
show = custom("K2Node_CustomEvent_0", "Show")
gt = call("K2Node_CallFunction_1", GS, "GetGameTimeInSeconds", [("WorldContextObject", "in", "object"), ("ReturnValue", "out", REAL)])
ts_ = call("K2Node_CallFunction_2", STRL, "Conv_DoubleToString", [("InDouble", "in", REAL, "0"), ("ReturnValue", "out", STR)])
fr = var_get("K2Node_VariableGet_0", "Frame", REAL)
fs_ = call("K2Node_CallFunction_3", STRL, "Conv_DoubleToString", [("InDouble", "in", REAL, "0"), ("ReturnValue", "out", STR)])
pa = print_("K2Node_CallFunction_4"); pb = print_("K2Node_CallFunction_5")
link(gt, "ReturnValue", ts_, "InDouble"); link(ts_, "ReturnValue", pa, "InString")
link(fr, "Frame", fs_, "InDouble"); link(fs_, "ReturnValue", pb, "InString")
link(show, "then", pa, "execute"); link(pa, "then", pb, "execute")
tm += [show, gt, ts_, fr, fs_, pa, pb]
write("time.txt", tm)

# --- random.txt: seeded random numbers in ranges.
rn = []
roll = custom("K2Node_CustomEvent_0", "Roll")
ri = call("K2Node_CallFunction_0", MATH, "RandomIntegerInRange", [("Min", "in", "int", "1"), ("Max", "in", "int", "6"), ("ReturnValue", "out", "int")])
si = var_set("K2Node_VariableSet_0", "Die", "int")
rf = call("K2Node_CallFunction_1", MATH, "RandomFloatInRange", [("Min", "in", REAL, "2.0"), ("Max", "in", REAL, "3.0"), ("ReturnValue", "out", REAL)])
sf = var_set("K2Node_VariableSet_1", "Spread", REAL)
r01 = call("K2Node_CallFunction_2", MATH, "RandomFloat", [("ReturnValue", "out", REAL)])
su = var_set("K2Node_VariableSet_2", "Unit", REAL)
link(roll, "then", si, "execute"); link(ri, "ReturnValue", si, "Die")
link(si, "then", sf, "execute"); link(rf, "ReturnValue", sf, "Spread")
link(sf, "then", su, "execute"); link(r01, "ReturnValue", su, "Unit")
rn += [roll, ri, si, rf, sf, r01, su]
rm = call("K2Node_CallFunction_3", MATH, "RandomInteger", [("Max", "in", "int", "4"), ("ReturnValue", "out", "int")])
sm = var_set("K2Node_VariableSet_3", "Pick", "int")
rb = call("K2Node_CallFunction_4", MATH, "RandomBool", [("ReturnValue", "out", "bool")])
sb = var_set("K2Node_VariableSet_4", "Coin", "bool")
link(su, "then", sm, "execute"); link(rm, "ReturnValue", sm, "Pick")
link(sm, "then", sb, "execute"); link(rb, "ReturnValue", sb, "Coin")
rn += [rm, sm, rb, sb]
write("random.txt", rn)

# --- parse.txt: numbers read from text.
pz = []
rd = custom("K2Node_CustomEvent_0", "Read", [("Text", STR)])
pi = call("K2Node_CallFunction_0", STRL, "Conv_StringToInt", [("InString", "in", STR), ("ReturnValue", "out", "int")])
sp = var_set("K2Node_VariableSet_0", "Whole", "int")
pf = call("K2Node_CallFunction_1", STRL, "Conv_StringToDouble", [("InString", "in", STR), ("ReturnValue", "out", REAL)])
sq = var_set("K2Node_VariableSet_1", "Real", REAL)
link(rd, "Text", pi, "InString"); link(rd, "Text", pf, "InString")
link(rd, "then", sp, "execute"); link(pi, "ReturnValue", sp, "Whole")
link(sp, "then", sq, "execute"); link(pf, "ReturnValue", sq, "Real")
pz += [rd, pi, sp, pf, sq]
write("parse.txt", pz)

# --- input.txt: key, input action and enhanced input action events.
GE = "/Script/InputBlueprintNodes."
ip = []
k1 = N(G + "K2Node_InputKey", "K2Node_InputKey_0", ["InputKey=SpaceBar"],
       [("Pressed", "out", EXEC), ("Released", "out", EXEC), ("Key", "out", "struct")])
pj = print_("K2Node_CallFunction_0", "jump"); pl = print_("K2Node_CallFunction_1", "land")
link(k1, "Pressed", pj, "execute"); link(k1, "Released", pl, "execute")
a1 = N(G + "K2Node_InputAction", "K2Node_InputAction_0", ['InputActionName="Fire"'],
       [("Pressed", "out", EXEC), ("Released", "out", EXEC), ("Key", "out", "struct")])
pb = print_("K2Node_CallFunction_2", "bang")
link(a1, "Pressed", pb, "execute")
e1 = N(GE + "K2Node_EnhancedInputAction", "K2Node_EnhancedInputAction_0",
       ["InputAction=/Script/EnhancedInput.InputAction'\"/Game/Input/IA_Move.IA_Move\"'"],
       [("Triggered", "out", EXEC), ("Started", "out", EXEC), ("Ongoing", "out", EXEC), ("Canceled", "out", EXEC), ("Completed", "out", EXEC)])
pm = print_("K2Node_CallFunction_3", "move")
link(e1, "Triggered", pm, "execute")
k2 = N(G + "K2Node_InputKey", "K2Node_InputKey_1", ["InputKey=SpaceBar"],
       [("Pressed", "out", EXEC), ("Released", "out", EXEC), ("Key", "out", "struct")])
ip += [k1, pj, pl, a1, pb, e1, pm, k2]
write("input.txt", ip)

# --- hide.txt: Set Actor Hidden In Game on self.
hd = []
hev = custom("K2Node_CustomEvent_0", "Hide")
hs = call("K2Node_CallFunction_0", "MemberParent=/Script/CoreUObject.Class'\"/Script/Engine.Actor\"'", "SetActorHiddenInGame",
          [("self", "in", "object"), ("bNewHidden", "in", BOOL, "true")], pure=False)
link(hev, "then", hs, "execute")
sev = custom("K2Node_CustomEvent_1", "Show")
ss = call("K2Node_CallFunction_1", "MemberParent=/Script/CoreUObject.Class'\"/Script/Engine.Actor\"'", "SetActorHiddenInGame",
          [("self", "in", "object"), ("bNewHidden", "in", BOOL, "false")], pure=False)
link(sev, "then", ss, "execute")
hd += [hev, hs, sev, ss]
write("hide.txt", hd)

# --- format.txt: Format Text with text, number and bool arguments.
fe = custom("K2Node_CustomEvent_0", "Show", [("Name", STR), ("Count", INT), ("Ratio", REAL), ("Rich", BOOL)])
ft = N(G + "K2Node_FormatText", "K2Node_FormatText_0", [],
       [("Format", "in", "text", 'text:NSLOCTEXT("[Seal]", "F1", "{Name} has {Count} coins ({Ratio}), rich: {Rich} `{x`} {Missing} - {Name}!")'),
        ("Result", "out", "text"), ("Name", "in", STR), ("Count", "in", INT), ("Ratio", "in", REAL), ("Rich", "in", BOOL)])
fp = print_("K2Node_CallFunction_0")
link(fe, "then", fp, "execute")
for a in ["Name", "Count", "Ratio", "Rich"]:
    link(fe, a, ft, a)
link(ft, "Result", fp, "InString")
ge2 = custom("K2Node_CustomEvent_1", "Plain")
ft2 = N(G + "K2Node_FormatText", "K2Node_FormatText_1", [],
        [("Format", "in", "text", 'text:INVTEXT("just text, {Unset}.")'), ("Result", "out", "text"), ("Unset", "in", "wildcard")])
fp2 = print_("K2Node_CallFunction_1")
link(ge2, "then", fp2, "execute")
link(ft2, "Result", fp2, "InString")
write("format.txt", [fe, ft, fp, ge2, ft2, fp2])

# --- format_plural.txt: an argument modifier Seal does not read.
pe = custom("K2Node_CustomEvent_0", "Show", [("N", INT)])
pt = N(G + "K2Node_FormatText", "K2Node_FormatText_0", [],
       [("Format", "in", "text", 'text:NSLOCTEXT("[Seal]", "F2", "{N} {N}|plural(one=coin,other=coins)")'), ("Result", "out", "text"), ("N", "in", INT)])
pp = print_("K2Node_CallFunction_0")
link(pe, "then", pp, "execute")
link(pe, "N", pt, "N")
link(pt, "Result", pp, "InString")
write("format_plural.txt", [pe, pt, pp])

# --- numtext.txt: number and bool to text, default look and one changed.
TXT = "MemberParent=/Script/CoreUObject.Class'\"/Script/Engine.KismetTextLibrary\"'"
ne = custom("K2Node_CustomEvent_0", "Show", [("I", INT), ("F", REAL), ("B", BOOL)])
it = call("K2Node_CallFunction_0", TXT, "Conv_IntToText", [("Value", "in", "int64", "0"), ("bAlwaysSign", "in", BOOL, "false"), ("bUseGrouping", "in", BOOL, "true"),
          ("MinimumIntegralDigits", "in", INT, "1"), ("MaximumIntegralDigits", "in", INT, "324"), ("ReturnValue", "out", "text")])
dt = call("K2Node_CallFunction_1", TXT, "Conv_DoubleToText", [("Value", "in", REAL, "0.0"), ("RoundingMode", "in", "byte", "HalfToEven"), ("bAlwaysSign", "in", BOOL, "false"),
          ("bUseGrouping", "in", BOOL, "true"), ("MinimumIntegralDigits", "in", INT, "1"), ("MaximumIntegralDigits", "in", INT, "324"),
          ("MinimumFractionalDigits", "in", INT, "0"), ("MaximumFractionalDigits", "in", INT, "3"), ("ReturnValue", "out", "text")])
bt = call("K2Node_CallFunction_2", TXT, "Conv_BoolToText", [("InBool", "in", BOOL, "false"), ("ReturnValue", "out", "text")])
nft = N(G + "K2Node_FormatText", "K2Node_FormatText_0", [],
        [("Format", "in", "text", 'text:INVTEXT("{A}/{B}/{C}")'), ("Result", "out", "text"), ("A", "in", "text"), ("B", "in", "text"), ("C", "in", "text")])
np_ = print_("K2Node_CallFunction_3")
link(ne, "then", np_, "execute")
link(ne, "I", it, "Value"); link(ne, "F", dt, "Value"); link(ne, "B", bt, "InBool")
link(it, "ReturnValue", nft, "A"); link(dt, "ReturnValue", nft, "B"); link(bt, "ReturnValue", nft, "C")
link(nft, "Result", np_, "InString")
write("numtext.txt", [ne, it, dt, bt, nft, np_])
padded = call("K2Node_CallFunction_0", TXT, "Conv_IntToText", [("Value", "in", "int64", "7"), ("MinimumIntegralDigits", "in", INT, "3"), ("ReturnValue", "out", "text")])
write("numtext_padded.txt", [padded])

# --- player.txt: Get Player Pawn and Get Player Character, checked with Is Valid.
we = custom("K2Node_CustomEvent_0", "Who")
pawn = call("K2Node_CallFunction_0", GS, "GetPlayerPawn", [("WorldContextObject", "in", "object"), ("PlayerIndex", "in", INT, "0"), ("ReturnValue", "out", "object")])
chr_ = call("K2Node_CallFunction_1", GS, "GetPlayerCharacter", [("WorldContextObject", "in", "object"), ("PlayerIndex", "in", INT, "1"), ("ReturnValue", "out", "object")])
v1 = call("K2Node_CallFunction_2", SYS, "IsValid", [("Object", "in", "object"), ("ReturnValue", "out", BOOL)])
v2 = call("K2Node_CallFunction_3", SYS, "IsValid", [("Object", "in", "object"), ("ReturnValue", "out", BOOL)])
s1 = call("K2Node_CallFunction_4", STRL, "Conv_BoolToString", [("InBool", "in", BOOL, "false"), ("ReturnValue", "out", STR)])
s2 = call("K2Node_CallFunction_5", STRL, "Conv_BoolToString", [("InBool", "in", BOOL, "false"), ("ReturnValue", "out", STR)])
cc = call("K2Node_CallFunction_6", STRL, "Concat_StrStr", [("A", "in", STR, ""), ("B", "in", STR, ""), ("ReturnValue", "out", STR)])
wp = print_("K2Node_CallFunction_7")
link(we, "then", wp, "execute")
link(pawn, "ReturnValue", v1, "Object"); link(chr_, "ReturnValue", v2, "Object")
link(v1, "ReturnValue", s1, "InBool"); link(v2, "ReturnValue", s2, "InBool")
link(s1, "ReturnValue", cc, "A"); link(s2, "ReturnValue", cc, "B")
link(cc, "ReturnValue", wp, "InString")
write("player.txt", [we, pawn, chr_, v1, v2, s1, s2, cc, wp])

# --- controller.txt: Get Player Controller, then the pawn it controls.
ce = custom("K2Node_CustomEvent_0", "Who")
pc = call("K2Node_CallFunction_0", GS, "GetPlayerController", [("WorldContextObject", "in", "object"), ("PlayerIndex", "in", INT, "0"), ("ReturnValue", "out", "object")])
CTRL = "MemberParent=/Script/CoreUObject.Class'\"/Script/Engine.Controller\"'"
gp = call("K2Node_CallFunction_1", CTRL, "K2_GetPawn", [("self", "in", "object"), ("ReturnValue", "out", "object")])
cv = call("K2Node_CallFunction_2", SYS, "IsValid", [("Object", "in", "object"), ("ReturnValue", "out", BOOL)])
cs = call("K2Node_CallFunction_3", STRL, "Conv_BoolToString", [("InBool", "in", BOOL, "false"), ("ReturnValue", "out", STR)])
cw = print_("K2Node_CallFunction_4")
link(ce, "then", cw, "execute")
link(pc, "ReturnValue", gp, "self"); link(gp, "ReturnValue", cv, "Object")
link(cv, "ReturnValue", cs, "InBool"); link(cs, "ReturnValue", cw, "InString")
write("controller.txt", [ce, pc, gp, cv, cs, cw])

# --- fields.txt: display name, tag check and velocity of this object.
fe = custom("K2Node_CustomEvent_0", "Show")
dn = call("K2Node_CallFunction_0", SYS, "GetDisplayName", [("Object", "in", "object"), ("ReturnValue", "out", STR)])
ht = actor("K2Node_CallFunction_1", "ActorHasTag", [("Tag", "in", "name", "enemy"), ("ReturnValue", "out", BOOL)], pure=True)
hs = call("K2Node_CallFunction_2", STRL, "Conv_BoolToString", [("InBool", "in", BOOL, "false"), ("ReturnValue", "out", STR)])
fc = call("K2Node_CallFunction_3", STRL, "Concat_StrStr", [("A", "in", STR, ""), ("B", "in", STR, ""), ("ReturnValue", "out", STR)])
fp = print_("K2Node_CallFunction_4")
vel = actor("K2Node_CallFunction_5", "GetVelocity", [("ReturnValue", "out", V3)], pure=True)
fm = actor("K2Node_CallFunction_6", "K2_SetActorLocation", [("NewLocation", "in", V3, "0.000000,0.000000,0.000000"), *MOVE])
link(fe, "then", fp, "execute"); link(fp, "then", fm, "execute")
link(dn, "ReturnValue", fc, "A"); link(ht, "ReturnValue", hs, "InBool"); link(hs, "ReturnValue", fc, "B")
link(fc, "ReturnValue", fp, "InString"); link(vel, "ReturnValue", fm, "NewLocation")
write("fields.txt", [fe, dn, ht, hs, fc, fp, vel, fm])

# --- touch.txt: overlap begin and end, hit and destroyed.
ob = event("K2Node_Event_0", "ReceiveActorBeginOverlap", [("OtherActor", "out", "object")])
oe = event("K2Node_Event_1", "ReceiveActorEndOverlap", [("OtherActor", "out", "object")])
HITP = [("MyComp", "out", "object"), ("Other", "out", "object"), ("OtherComp", "out", "object"), ("bSelfMoved", "out", BOOL),
        ("HitLocation", "out", V3), ("HitNormal", "out", V3), ("NormalImpulse", "out", V3), ("Hit", "out", "struct")]
oh = event("K2Node_Event_2", "ReceiveHit", HITP)
od = event("K2Node_Event_3", "ReceiveDestroyed")
tk = destroy("K2Node_CallFunction_0")
tp1 = print_("K2Node_CallFunction_1", "end")
tp2 = print_("K2Node_CallFunction_2", "hit")
tp3 = print_("K2Node_CallFunction_3", "gone")
link(ob, "then", tk, "execute"); link(ob, "OtherActor", tk, "self")
link(oe, "then", tp1, "execute"); link(oh, "then", tp2, "execute"); link(od, "then", tp3, "execute")
write("touch.txt", [ob, oe, oh, od, tk, tp1, tp2, tp3])
hb = event("K2Node_Event_0", "ReceiveHit", HITP)
hbp = print_("K2Node_CallFunction_0")
hbs = call("K2Node_CallFunction_1", STRL, "Conv_VectorToString", [("InVec", "in", V3, ""), ("ReturnValue", "out", STR)])
link(hb, "then", hbp, "execute"); link(hb, "NormalImpulse", hbs, "InVec"); link(hbs, "ReturnValue", hbp, "InString")
write("touch_bad.txt", [hb, hbp, hbs])
# --- touch_hit.txt: the hit location and normal are carried.
hh = event("K2Node_Event_0", "ReceiveHit", HITP)
hp1 = print_("K2Node_CallFunction_0")
hp2 = print_("K2Node_CallFunction_1")
hs1 = call("K2Node_CallFunction_2", STRL, "Conv_VectorToString", [("InVec", "in", V3, ""), ("ReturnValue", "out", STR)])
hs2 = call("K2Node_CallFunction_3", STRL, "Conv_VectorToString", [("InVec", "in", V3, ""), ("ReturnValue", "out", STR)])
link(hh, "then", hp1, "execute"); link(hp1, "then", hp2, "execute")
link(hh, "HitLocation", hs1, "InVec"); link(hs1, "ReturnValue", hp1, "InString")
link(hh, "HitNormal", hs2, "InVec"); link(hs2, "ReturnValue", hp2, "InString")
write("touch_hit.txt", [hh, hp1, hp2, hs1, hs2])

# --- interp.txt: FInterpTo, VInterpTo, MapRangeClamped and GetDistanceTo.
def fstr(name):
    return call(name, STRL, "Conv_DoubleToString", [("InDouble", "in", REAL), ("ReturnValue", "out", STR)])
def vstr(name):
    return call(name, STRL, "Conv_VectorToString", [("InVec", "in", V3, ""), ("ReturnValue", "out", STR)])
ie = custom("K2Node_CustomEvent_0", "Interp", [("C", REAL), ("T", REAL), ("Dt", REAL), ("Sp", REAL)])
fi = call("K2Node_CallFunction_0", MATH, "FInterpTo", [("Current", "in", REAL), ("Target", "in", REAL), ("DeltaTime", "in", REAL), ("InterpSpeed", "in", REAL), ("ReturnValue", "out", REAL)])
ip, is_ = print_("K2Node_CallFunction_1"), fstr("K2Node_CallFunction_2")
link(ie, "then", ip, "execute"); link(fi, "ReturnValue", is_, "InDouble"); link(is_, "ReturnValue", ip, "InString")
for a, b in [("C", "Current"), ("T", "Target"), ("Dt", "DeltaTime"), ("Sp", "InterpSpeed")]:
    link(ie, a, fi, b)
me = custom("K2Node_CustomEvent_1", "Map", [("V", REAL), ("A", REAL), ("B", REAL)])
mr = call("K2Node_CallFunction_3", MATH, "MapRangeClamped", [("Value", "in", REAL), ("InRangeA", "in", REAL), ("InRangeB", "in", REAL), ("OutRangeA", "in", REAL, "10.000000"), ("OutRangeB", "in", REAL, "20.000000"), ("ReturnValue", "out", REAL)])
mp, ms = print_("K2Node_CallFunction_4"), fstr("K2Node_CallFunction_5")
link(me, "then", mp, "execute"); link(me, "V", mr, "Value"); link(me, "A", mr, "InRangeA"); link(me, "B", mr, "InRangeB")
link(mr, "ReturnValue", ms, "InDouble"); link(ms, "ReturnValue", mp, "InString")
ve = custom("K2Node_CustomEvent_2", "VI", [("C", V3), ("T", V3)])
vi = call("K2Node_CallFunction_6", MATH, "VInterpTo", [("Current", "in", V3, ""), ("Target", "in", V3, ""), ("DeltaTime", "in", REAL, "0.500000"), ("InterpSpeed", "in", REAL, "1.000000"), ("ReturnValue", "out", V3)])
vp, vs = print_("K2Node_CallFunction_7"), vstr("K2Node_CallFunction_8")
link(ve, "then", vp, "execute"); link(ve, "C", vi, "Current"); link(ve, "T", vi, "Target")
link(vi, "ReturnValue", vs, "InVec"); link(vs, "ReturnValue", vp, "InString")
de = custom("K2Node_CustomEvent_3", "Dist", [("Other", "object")])
dd = actor("K2Node_CallFunction_9", "GetDistanceTo", [("OtherActor", "in", "object"), ("ReturnValue", "out", REAL)], pure=True)
dp, ds = print_("K2Node_CallFunction_10"), fstr("K2Node_CallFunction_11")
link(de, "then", dp, "execute"); link(de, "Other", dd, "OtherActor"); link(dd, "ReturnValue", ds, "InDouble"); link(ds, "ReturnValue", dp, "InString")
write("interp.txt", [ie, fi, ip, is_, me, mr, mp, ms, ve, vi, vp, vs, de, dd, dp, ds])

# --- axes.txt: forward, right and up vectors from a rotator and from the actor.
ROT = "struct"
ax_nodes = []
de = custom("K2Node_CustomEvent_0", "Axes", [("R", V3)])
ax_nodes.append(de)
prev, n = (de, "then"), 0
for member in ["GetForwardVector", "GetRightVector", "GetUpVector"]:
    g = call(f"K2Node_CallFunction_{n}", MATH, member, [("InRot", "in", V3, ""), ("ReturnValue", "out", V3)])
    p_, t = print_(f"K2Node_CallFunction_{n+1}"), vstr(f"K2Node_CallFunction_{n+2}")
    n += 3
    link(prev[0], prev[1], p_, "execute"); link(de, "R", g, "InRot"); link(g, "ReturnValue", t, "InVec"); link(t, "ReturnValue", p_, "InString")
    prev = (p_, "then"); ax_nodes += [g, p_, t]
se = custom("K2Node_CustomEvent_1", "Mine", [])
ax_nodes.append(se)
prev = (se, "then")
for member in ["GetActorForwardVector", "GetActorRightVector", "GetActorUpVector"]:
    g = actor(f"K2Node_CallFunction_{n}", member, [("ReturnValue", "out", V3)], pure=True)
    p_, t = print_(f"K2Node_CallFunction_{n+1}"), vstr(f"K2Node_CallFunction_{n+2}")
    n += 3
    link(prev[0], prev[1], p_, "execute"); link(g, "ReturnValue", t, "InVec"); link(t, "ReturnValue", p_, "InString")
    prev = (p_, "then"); ax_nodes += [g, p_, t]
write("axes.txt", ax_nodes)

# --- fmod.txt: float % and the FMod node.
def istr(name):
    return call(name, STRL, "Conv_IntToString", [("InInt", "in", INT), ("ReturnValue", "out", STR)])
me = custom("K2Node_CustomEvent_0", "Mod", [("A", REAL), ("B", REAL)])
pc = call("K2Node_CallFunction_0", MATH, "Percent_FloatFloat", [("A", "in", REAL), ("B", "in", REAL), ("ReturnValue", "out", REAL)])
fm = call("K2Node_CallFunction_1", MATH, "FMod", [("Dividend", "in", REAL), ("Divisor", "in", REAL), ("Remainder", "out", REAL), ("ReturnValue", "out", INT)])
p1, s1 = print_("K2Node_CallFunction_2"), fstr("K2Node_CallFunction_3")
p2, s2 = print_("K2Node_CallFunction_4"), istr("K2Node_CallFunction_5")
p3, s3 = print_("K2Node_CallFunction_6"), fstr("K2Node_CallFunction_7")
link(me, "A", pc, "A"); link(me, "B", pc, "B"); link(me, "A", fm, "Dividend"); link(me, "B", fm, "Divisor")
link(me, "then", p1, "execute"); link(p1, "then", p2, "execute"); link(p2, "then", p3, "execute")
link(pc, "ReturnValue", s1, "InDouble"); link(s1, "ReturnValue", p1, "InString")
link(fm, "ReturnValue", s2, "InInt"); link(s2, "ReturnValue", p2, "InString")
link(fm, "Remainder", s3, "InDouble"); link(s3, "ReturnValue", p3, "InString")
write("fmod.txt", [me, pc, fm, p1, s1, p2, s2, p3, s3])

# --- self.txt: a Self node feeding Get Display Name.
se = custom("K2Node_CustomEvent_0", "Me")
sn = N(G + "K2Node_Self", "K2Node_Self_0", [], [("self", "out", "object")])
sd = call("K2Node_CallFunction_0", SYS, "GetDisplayName", [("Object", "in", "object"), ("ReturnValue", "out", STR)])
spr = print_("K2Node_CallFunction_1")
link(se, "then", spr, "execute"); link(sn, "self", sd, "Object"); link(sd, "ReturnValue", spr, "InString")
write("self.txt", [se, sn, sd, spr])

# --- enums.txt: Switch on Enum and enum == by enumerator name.
ee = custom("K2Node_CustomEvent_0", "Feel", [("M", "enum")])
sw = N(G + "K2Node_SwitchEnum", "K2Node_SwitchEnum_0", ["Enum=\"/Script/Engine.UserDefinedEnum'/Game/Mood.Mood'\""],
       [("execute", "in", EXEC), ("Selection", "in", "enum", "NewEnumerator0"),
        ("NewEnumerator0", "out", EXEC), ("NewEnumerator1", "out", EXEC), ("NewEnumerator2", "out", EXEC)])
eq = N(G + "K2Node_EnumEquality", "K2Node_EnumEquality_0", [], [("A", "in", "enum"), ("B", "in", "enum", "NewEnumerator2"), ("ReturnValue", "out", BOOL)])
ep0 = print_("K2Node_CallFunction_0", "calm")
ep1 = print_("K2Node_CallFunction_1", "glad")
ep2 = print_("K2Node_CallFunction_2")
ec = call("K2Node_CallFunction_3", "/Script/Engine.KismetStringLibrary", "Conv_BoolToString", [("InBool", "in", BOOL), ("ReturnValue", "out", STR)])
link(ee, "then", sw, "execute"); link(ee, "M", sw, "Selection")
link(sw, "NewEnumerator0", ep0, "execute"); link(sw, "NewEnumerator1", ep1, "execute"); link(sw, "NewEnumerator2", ep2, "execute")
link(ee, "M", eq, "A"); link(eq, "ReturnValue", ec, "InBool"); link(ec, "ReturnValue", ep2, "InString")
write("enums.txt", [ee, sw, eq, ep0, ep1, ep2, ec])

# --- structs.txt: Make Struct and Break Struct on vectors and rotations.
VEC = "StructType=/Script/CoreUObject.ScriptStruct'\"/Script/CoreUObject.Vector\"'"
ROT = "StructType=/Script/CoreUObject.ScriptStruct'\"/Script/CoreUObject.Rotator\"'"
V2D = "StructType=/Script/CoreUObject.ScriptStruct'\"/Script/CoreUObject.Vector2D\"'"
COL = "StructType=/Script/CoreUObject.ScriptStruct'\"/Script/CoreUObject.LinearColor\"'"
se = custom("K2Node_CustomEvent_0", "Go", [("A", REAL), ("B", REAL)])
mk = N(G + "K2Node_MakeStruct", "K2Node_MakeStruct_0", [VEC], [("Vector", "out", "vector"), ("X", "in", REAL, "0.0"), ("Y", "in", REAL, "5.0"), ("Z", "in", REAL, "0.0")])
bk = N(G + "K2Node_BreakStruct", "K2Node_BreakStruct_0", [VEC], [("Vector", "in", "vector"), ("X", "out", REAL), ("Y", "out", REAL), ("Z", "out", REAL)])
mr = N(G + "K2Node_MakeStruct", "K2Node_MakeStruct_1", [ROT], [("Rotator", "out", "rotator"), ("Pitch", "in", REAL, "0.0"), ("Yaw", "in", REAL, "90.0"), ("Roll", "in", REAL, "0.0")])
br = N(G + "K2Node_BreakStruct", "K2Node_BreakStruct_1", [ROT], [("Rotator", "in", "rotator"), ("Pitch", "out", REAL), ("Yaw", "out", REAL), ("Roll", "out", REAL)])
m2 = N(G + "K2Node_MakeStruct", "K2Node_MakeStruct_2", [V2D], [("Vector2D", "out", "struct"), ("X", "in", REAL, "0.0"), ("Y", "in", REAL, "0.0")])
b2 = N(G + "K2Node_BreakStruct", "K2Node_BreakStruct_2", [V2D], [("Vector2D", "in", "struct"), ("X", "out", REAL), ("Y", "out", REAL)])
sv = var_set("K2Node_VariableSet_0", "Sum", REAL)
add3 = call("K2Node_CallFunction_0", MATH, "Add_DoubleDouble", [("A", "in", REAL, "0.0"), ("B", "in", REAL, "0.0"), ("ReturnValue", "out", REAL)])
add4 = call("K2Node_CallFunction_1", MATH, "Add_DoubleDouble", [("A", "in", REAL, "0.0"), ("B", "in", REAL, "0.0"), ("ReturnValue", "out", REAL)])
add5 = call("K2Node_CallFunction_2", MATH, "Add_DoubleDouble", [("A", "in", REAL, "0.0"), ("B", "in", REAL, "0.0"), ("ReturnValue", "out", REAL)])
link(se, "then", sv, "execute")
link(se, "A", mk, "X"); link(se, "B", mr, "Roll"); link(se, "A", m2, "Y")
link(mk, "Vector", bk, "Vector"); link(mr, "Rotator", br, "Rotator"); link(m2, "Vector2D", b2, "Vector2D")
# Sum = (X + Y) + (Yaw + Roll) + 2D Y: A + 5 + 90 + B + A.
link(bk, "X", add3, "A"); link(bk, "Y", add3, "B")
link(br, "Yaw", add4, "A"); link(br, "Roll", add4, "B")
link(add3, "ReturnValue", add5, "A"); link(add4, "ReturnValue", add5, "B")
add6 = call("K2Node_CallFunction_3", MATH, "Add_DoubleDouble", [("A", "in", REAL, "0.0"), ("B", "in", REAL, "0.0"), ("ReturnValue", "out", REAL)])
link(add5, "ReturnValue", add6, "A"); link(b2, "Y", add6, "B")
link(add6, "ReturnValue", sv, "Sum")
write("structs.txt", [se, mk, bk, mr, br, m2, b2, add3, add4, add5, add6, sv])
write("structs_color.txt", [N(G + "K2Node_MakeStruct", "K2Node_MakeStruct_0", [COL], [("LinearColor", "out", "struct"), ("R", "in", REAL, "1.0")])])

# --- timeline.txt: a timeline driven by custom events, a looping autoplay
# timeline with a vector track, and engine BeginPlay and Tick alongside.
TLD = "enum"
def timeline(name, tl, tracks, loop=False, auto=False):
    props = [f'TimelineName="{tl}"', f"bAutoPlay={auto}", f"bLoop={loop}", f"TimelineGuid={guid(name, 'tl')}"]
    pins = [("Play", "in", EXEC), ("PlayFromStart", "in", EXEC), ("Stop", "in", EXEC), ("Reverse", "in", EXEC),
            ("ReverseFromEnd", "in", EXEC), ("Update", "out", EXEC), ("Finished", "out", EXEC), ("SetNewTime", "in", EXEC),
            ("NewTime", "in", REAL, "0.0"), ("Direction", "out", TLD), *[(t, "out", ty) for t, ty in tracks]]
    return N(G + "K2Node_Timeline", name, props, pins)
t1 = timeline("K2Node_Timeline_0", "Fade", [("Alpha", REAL)])
go, back, halt = custom("K2Node_CustomEvent_0", "Go"), custom("K2Node_CustomEvent_1", "Back"), custom("K2Node_CustomEvent_2", "Halt")
jump = custom("K2Node_CustomEvent_3", "Jump", [("T", REAL)])
again = custom("K2Node_CustomEvent_4", "Again")
link(go, "then", t1, "Play"); link(back, "then", t1, "ReverseFromEnd"); link(halt, "then", t1, "Stop")
link(jump, "then", t1, "SetNewTime"); link(jump, "T", t1, "NewTime"); link(again, "then", t1, "PlayFromStart")
up = var_set("K2Node_VariableSet_0", "Level", REAL)
link(t1, "Update", up, "execute"); link(t1, "Alpha", up, "Level")
dirset = var_set("K2Node_VariableSet_1", "Way", TLD)
link(up, "then", dirset, "execute"); link(t1, "Direction", dirset, "Way")
done = print_("K2Node_CallFunction_0", "done")
link(t1, "Finished", done, "execute")
t2 = timeline("K2Node_Timeline_1", "Spin", [("Pos", "vector")], loop=True, auto=True)
sp = var_set("K2Node_VariableSet_2", "Where", "vector")
link(t2, "Update", sp, "execute"); link(t2, "Pos", sp, "Where")
begin = event("K2Node_Event_0", "ReceiveBeginPlay")
hi = print_("K2Node_CallFunction_1", "begin")
link(begin, "then", hi, "execute")
tk = event("K2Node_Event_1", "ReceiveTick", [("DeltaSeconds", "out", REAL)])
gt = var_get("K2Node_VariableGet_0", "Ticks", REAL)
add1 = math("K2Node_CallFunction_2", "Add_DoubleDouble", REAL, a="0.0", b="1.0")
st = var_set("K2Node_VariableSet_3", "Ticks", REAL)
link(tk, "then", st, "execute"); link(gt, "Ticks", add1, "A"); link(add1, "ReturnValue", st, "Ticks")
write("timeline.txt", [t1, go, back, halt, jump, again, up, dirset, done, t2, sp, begin, hi, tk, gt, add1, st])

# --- texts.txt: text helpers: empty check, case-sensitive and ignore-case
# compares, upper and lower case, and trimming of texts and strings.
TXT = "text"
tev = custom("K2Node_CustomEvent_0", "T", [("S", TXT)])
tx_nodes, tx_prints = [tev], []
def bool_str(tag, src, pin):
    b = call(f"K2Node_CallFunction_b{tag}", STRL, "Conv_BoolToString", [("InBool", "in", BOOL, "false"), ("ReturnValue", "out", STR)])
    link(src, pin, b, "InBool"); return b
for k, (member, extra, out) in enumerate([
        ("TextIsEmpty", [], BOOL), ("EqualEqual_TextText", [("B", "in", TXT, 'text:INVTEXT("Hi")')], BOOL),
        ("NotEqual_TextText", [("B", "in", TXT, 'text:INVTEXT("Hi")')], BOOL),
        ("EqualEqual_IgnoreCase_TextText", [("B", "in", TXT, 'text:INVTEXT("hi")')], BOOL),
        ("TextToUpper", [], TXT), ("TextToLower", [], TXT), ("TextTrimPreceding", [], TXT),
        ("TextTrimTrailing", [], TXT), ("TextTrimPrecedingAndTrailing", [], TXT), ("Trim", [], STR), ("TrimTrailing", [], STR)]):
    lib = "MemberParent=/Script/CoreUObject.Class'\"/Script/Engine.KismetTextLibrary\"'" if member.startswith(("Text", "EqualEqual", "NotEqual")) else STRL
    inpin = "SourceString" if member in ("Trim", "TrimTrailing") else ("A" if "Equal" in member else "InText")
    c = call(f"K2Node_CallFunction_{k}", lib, member, [(inpin, "in", STR if inpin == "SourceString" else TXT), *extra, ("ReturnValue", "out", out)])
    link(tev, "S", c, inpin)
    pr = print_(f"K2Node_CallFunction_p{k}")
    if out == BOOL:
        b = bool_str(k, c, "ReturnValue"); link(b, "ReturnValue", pr, "InString"); tx_nodes += [c, b, pr]
    else:
        link(c, "ReturnValue", pr, "InString"); tx_nodes += [c, pr]
    tx_prints.append(pr)
chain(tev, "then", tx_prints)
write("texts.txt", tx_nodes)

# --- timer_handles.txt: timers set by event (a custom event's own delegate
# and a Create Event node), handles kept in variables, cleared by handle.
TH = "timerhandle"
def set_timer_ev(name, secs, loop):
    return call(name, SYS, "K2_SetTimerDelegate", [("Event", "in", "delegate"), ("Time", "in", REAL, secs), ("bLooping", "in", BOOL, loop),
                ("InitialStartDelay", "in", REAL, "0.000000"), ("InitialStartDelayVariance", "in", REAL, "0.000000"), ("ReturnValue", "out", TH)], pure=False)
tk_ev = custom("K2Node_CustomEvent_0", "Tick")
tk_p = print_("K2Node_CallFunction_0", "tick"); link(tk_ev, "then", tk_p, "execute")
bt_ev = custom("K2Node_CustomEvent_1", "Beat")
bt_p = print_("K2Node_CallFunction_1", "beat"); link(bt_ev, "then", bt_p, "execute")
sa = custom("K2Node_CustomEvent_2", "StartA")
sa_t = set_timer_ev("K2Node_CallFunction_2", "1.000000", "true")
sa_h = var_set("K2Node_VariableSet_0", "H", TH)
link(sa, "then", sa_t, "execute"); link(tk_ev, "OutputDelegate", sa_t, "Event"); link(sa_t, "then", sa_h, "execute"); link(sa_t, "ReturnValue", sa_h, "H")
sb = custom("K2Node_CustomEvent_3", "StartB")
cd = N(G + "K2Node_CreateDelegate", "K2Node_CreateDelegate_0", ['SelectedFunctionName="Beat"'], [("self", "in", "object"), ("OutputDelegate", "out", "delegate")])
sb_t = set_timer_ev("K2Node_CallFunction_3", "0.500000", "false")
sb_h = var_set("K2Node_VariableSet_1", "H2", TH)
link(sb, "then", sb_t, "execute"); link(cd, "OutputDelegate", sb_t, "Event"); link(sb_t, "then", sb_h, "execute"); link(sb_t, "ReturnValue", sb_h, "H2")
xa = custom("K2Node_CustomEvent_4", "StopA")
xa_c = call("K2Node_CallFunction_4", SYS, "K2_ClearAndInvalidateTimerHandle", [("WorldContextObject", "in", "object"), ("Handle", "in", TH)], pure=False)
xa_g = var_get("K2Node_VariableGet_0", "H", TH)
xa_g2 = var_get("K2Node_VariableGet_1", "H", TH)
xa_p = print_("K2Node_CallFunction_5")
link(xa, "then", xa_c, "execute"); link(xa_g, "H", xa_c, "Handle"); link(xa_c, "then", xa_p, "execute"); link(xa_g2, "H", xa_p, "InString")
xb = custom("K2Node_CustomEvent_5", "StopB")
xb_c = call("K2Node_CallFunction_6", SYS, "K2_ClearTimerHandle", [("WorldContextObject", "in", "object"), ("Handle", "in", TH)], pure=False)
xb_g = var_get("K2Node_VariableGet_2", "H2", TH)
link(xb, "then", xb_c, "execute"); link(xb_g, "H2", xb_c, "Handle")
write("timer_handles.txt", [tk_ev, tk_p, bt_ev, bt_p, sa, sa_t, sa_h, sb, cd, sb_t, sb_h, xa, xa_c, xa_g, xa_g2, xa_p, xb, xb_c, xb_g])

# --- movement.txt: an input axis event pushing movement input, the tick
# reading another axis, and Jump / StopJumping.
ax = N(G + "K2Node_InputAxisEvent", "K2Node_InputAxisEvent_0", ['InputAxisName="MoveForward"'],
       [("OutputDelegate", "out", "delegate"), ("then", "out", EXEC), ("AxisValue", "out", REAL)])
fw = actor("K2Node_CallFunction_0", "AddMovementInput", [("WorldDirection", "in", "vector", "1.000000,0.000000,0.000000"), ("ScaleValue", "in", REAL), ("bForce", "in", BOOL, "false")])
link(ax, "then", fw, "execute"); link(ax, "AxisValue", fw, "ScaleValue")
mt = event("K2Node_Event_0", "ReceiveTick", [("DeltaSeconds", "out", REAL)])
rt_ = actor("K2Node_CallFunction_1", "AddMovementInput", [("WorldDirection", "in", "vector", "0.000000,1.000000,0.000000"), ("ScaleValue", "in", REAL), ("bForce", "in", BOOL, "false")])
rv = actor("K2Node_CallFunction_2", "GetInputAxisValue", [("InputAxisName", "in", "name", "MoveRight"), ("ReturnValue", "out", REAL)], pure=True)
link(mt, "then", rt_, "execute"); link(rv, "ReturnValue", rt_, "ScaleValue")
hop = custom("K2Node_CustomEvent_0", "Hop")
jp = actor("K2Node_CallFunction_3", "Jump", [])
sj = actor("K2Node_CallFunction_4", "StopJumping", [])
hp = print_("K2Node_CallFunction_5", "hopped")
link(hop, "then", jp, "execute"); link(jp, "then", sj, "execute"); link(sj, "then", hp, "execute")
write("movement.txt", [ax, fw, mt, rt_, rv, hop, jp, sj, hp])

# --- attach.txt: attach to an object with each location rule, read the
# parent back, detach, and an unsupported socket attach.
RULES = lambda loc: [("LocationRule", "in", "byte", loc), ("RotationRule", "in", "byte", "KeepRelative"), ("ScaleRule", "in", "byte", "KeepRelative"), ("bWeldSimulatedBodies", "in", BOOL, "false")]
at_ev = custom("K2Node_CustomEvent_0", "Ride", [("Horse", "object")])
at_a = actor("K2Node_CallFunction_0", "AttachActorToActor", [("ParentActor", "in", "object"), ("SocketName", "in", "name", "None"), *RULES("KeepWorld")])
at_g = actor("K2Node_CallFunction_1", "GetAttachParentActor", [("ReturnValue", "out", "object")], pure=True)
at_v = math("K2Node_CallFunction_2", "EqualEqual_ObjectObject", "object", BOOL)
at_s = call("K2Node_CallFunction_3", STRL, "Conv_BoolToString", [("InBool", "in", BOOL), ("ReturnValue", "out", STR)])
at_p = print_("K2Node_CallFunction_4")
link(at_ev, "then", at_a, "execute"); link(at_ev, "Horse", at_a, "ParentActor"); link(at_a, "then", at_p, "execute")
link(at_g, "ReturnValue", at_v, "A"); link(at_ev, "Horse", at_v, "B"); link(at_v, "ReturnValue", at_s, "InBool"); link(at_s, "ReturnValue", at_p, "InString")
sn_ev = custom("K2Node_CustomEvent_1", "Snap", [("Horse", "object")])
sn_a = actor("K2Node_CallFunction_5", "K2_AttachToActor", [("ParentActor", "in", "object"), ("SocketName", "in", "name", "None"), *RULES("SnapToTarget")])
link(sn_ev, "then", sn_a, "execute"); link(sn_ev, "Horse", sn_a, "ParentActor")
dt_ev = custom("K2Node_CustomEvent_2", "GetOff")
dt_a = actor("K2Node_CallFunction_6", "K2_DetachFromActor", [("LocationRule", "in", "byte", "KeepWorld"), ("RotationRule", "in", "byte", "KeepWorld"), ("ScaleRule", "in", "byte", "KeepWorld")])
link(dt_ev, "then", dt_a, "execute")
write("attach.txt", [at_ev, at_a, at_g, at_v, at_s, at_p, sn_ev, sn_a, dt_ev, dt_a])

so_ev = custom("K2Node_CustomEvent_0", "Sock", [("Horse", "object")])
so_a = actor("K2Node_CallFunction_0", "AttachActorToActor", [("ParentActor", "in", "object"), ("SocketName", "in", "name", "Saddle"), *RULES("KeepWorld")])
link(so_ev, "then", so_a, "execute"); link(so_ev, "Horse", so_a, "ParentActor")
write("attach_socket.txt", [so_ev, so_a])

# --- sounds.txt: PlaySound2D with a chosen sound, PlaySoundAtLocation at the
# object's location, and SpawnSound2D whose component is not used.
SND = lambda path, vol="1.000000": [("WorldContextObject", "in", "object"), ("Sound", "in", "object", "obj:" + path),
                                   ("VolumeMultiplier", "in", REAL, vol), ("PitchMultiplier", "in", REAL, "1.000000"),
                                   ("StartTime", "in", REAL, "0.000000"), ("ConcurrencySettings", "in", "object"), ("OwningActor", "in", "object")]
sb_ev = custom("K2Node_CustomEvent_0", "Boom")
sb_p = call("K2Node_CallFunction_0", GS, "PlaySound2D", SND("/Game/Audio/Boom.Boom", "0.500000"), pure=False)
sb_l = actor("K2Node_CallFunction_1", "K2_GetActorLocation", [("ReturnValue", "out", "vector")], pure=True)
sb_a = call("K2Node_CallFunction_2", GS, "PlaySoundAtLocation", [*SND("/Game/Audio/Hit.Hit"), ("Location", "in", "vector"), ("Rotation", "in", "rotator"), ("AttenuationSettings", "in", "object")], pure=False)
link(sb_ev, "then", sb_p, "execute"); link(sb_p, "then", sb_a, "execute"); link(sb_l, "ReturnValue", sb_a, "Location")
sd_ev = custom("K2Node_CustomEvent_1", "Ding")
sd_s = call("K2Node_CallFunction_3", GS, "SpawnSound2D", [*SND("SoundWave'/Game/UI/Ding.Ding'"), ("bPersistAcrossLevelTransition", "in", BOOL, "false"), ("bAutoDestroy", "in", BOOL, "true"), ("ReturnValue", "out", "object")], pure=False)
link(sd_ev, "then", sd_s, "execute")
write("sounds.txt", [sb_ev, sb_p, sb_l, sb_a, sd_ev, sd_s])

ss_ev = custom("K2Node_CustomEvent_0", "Keep")
ss_s = call("K2Node_CallFunction_0", GS, "SpawnSound2D", [*SND("/Game/UI/Ding.Ding"), ("ReturnValue", "out", "object")], pure=False)
ss_v = var_set("K2Node_VariableSet_0", "Music", "object")
link(ss_ev, "then", ss_s, "execute"); link(ss_s, "then", ss_v, "execute"); link(ss_s, "ReturnValue", ss_v, "Music")
write("sounds_bad.txt", [ss_ev, ss_s, ss_v])

# --- widgets.txt: create a widget and add it to the screen, hide this
# graph's own widget, and set a text element read from a widget variable.
UW = "MemberParent=/Script/CoreUObject.Class'\"/Script/UMG.UserWidget\"'"
TB = "MemberParent=/Script/CoreUObject.Class'\"/Script/UMG.TextBlock\"'"
wg_b = event("K2Node_Event_0", "ReceiveBeginPlay")
wg_c = N(G + "K2Node_CreateWidget", "K2Node_CreateWidget_0", [],
         [("execute", "in", EXEC), ("then", "out", EXEC), ("Class", "in", "class", "obj:WidgetBlueprintGeneratedClass'/Game/UI/WBP_Hud.WBP_Hud_C'"),
          ("OwningPlayer", "in", "object"), ("ReturnValue", "out", "object")])
wg_a = call("K2Node_CallFunction_0", UW, "AddToViewport", [("ZOrder", "in", "int", "0")], pure=False)
link(wg_b, "then", wg_c, "execute"); link(wg_c, "then", wg_a, "execute"); link(wg_c, "ReturnValue", wg_a, "self")
wg_h = custom("K2Node_CustomEvent_0", "Hide")
wg_r = call("K2Node_CallFunction_1", UW, "RemoveFromParent", [], pure=False)
link(wg_h, "then", wg_r, "execute")
wg_s = custom("K2Node_CustomEvent_1", "Score", [("Label", "text")])
wg_v = var_get("K2Node_VariableGet_0", "ScoreText", "object")
wg_t = call("K2Node_CallFunction_2", TB, "SetText", [("InText", "in", "text")], pure=False)
link(wg_s, "then", wg_t, "execute"); link(wg_s, "Label", wg_t, "InText"); link(wg_v, "ScoreText", wg_t, "self")
write("widgets.txt", [wg_b, wg_c, wg_a, wg_h, wg_r, wg_s, wg_v, wg_t])
