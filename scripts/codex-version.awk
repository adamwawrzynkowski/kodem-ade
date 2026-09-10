# Exit successfully when candidate and minimum are valid semantic versions and
# candidate has at least minimum's precedence. Build metadata has no precedence.
function numeric(value) { return value ~ /^(0|[1-9][0-9]*)$/ }
function compare_number(a, b) {
    if (length(a) != length(b)) return length(a) > length(b) ? 1 : -1
    return ("x" a > "x" b) - ("x" a < "x" b)
}
function parse(value, core, pre, parts, count, i, suffix) {
    count = split(value, parts, /\+/)
    if (count > 2 || (count == 2 && parts[2] !~ /^[0-9A-Za-z-]+(\.[0-9A-Za-z-]+)*$/)) return 0
    value = parts[1]
    i = index(value, "-")
    if (i) {
        suffix = substr(value, i + 1)
        value = substr(value, 1, i - 1)
        if (suffix !~ /^[0-9A-Za-z-]+(\.[0-9A-Za-z-]+)*$/) return 0
    }
    if (split(value, core, /\./) != 3) return 0
    for (i = 1; i <= 3; i++) {
        if (!numeric(core[i]) || compare_number(core[i], "18446744073709551615") > 0) return 0
    }
    count = split(suffix, pre, /\./)
    for (i = 1; i <= count; i++) {
        if (pre[i] ~ /^[0-9]+$/ && !numeric(pre[i])) return 0
    }
    return 1
}
BEGIN {
    if (!parse(candidate, a, ap) || !parse(minimum, b, bp)) exit 1
    for (i = 1; i <= 3; i++) {
        order = compare_number(a[i], b[i])
        if (order) exit order < 0
    }
    if (!length(ap) || !length(bp)) exit length(ap) > length(bp)
    for (i = 1; i <= length(ap) && i <= length(bp); i++) {
        if ("x" ap[i] == "x" bp[i]) continue
        an = numeric(ap[i]); bn = numeric(bp[i])
        if (an != bn) exit an
        if (an) exit compare_number(ap[i], bp[i]) < 0
        exit "x" ap[i] < "x" bp[i]
    }
    exit length(ap) < length(bp)
}
