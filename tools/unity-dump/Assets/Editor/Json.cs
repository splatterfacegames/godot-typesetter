// Minimal JSON reader/writer for the dump harness (objects -> Dictionary<string, object>,
// arrays -> List<object>, numbers -> double). Written for Typesetter; no dependencies.

using System;
using System.Collections;
using System.Collections.Generic;
using System.Globalization;
using System.Text;

public static class Json
{
    public static object Parse(string s)
    {
        int i = 0;
        object v = Value(s, ref i);
        return v;
    }

    static void Ws(string s, ref int i)
    {
        while (i < s.Length && char.IsWhiteSpace(s[i])) i++;
    }

    static object Value(string s, ref int i)
    {
        Ws(s, ref i);
        char c = s[i];
        if (c == '{')
        {
            var d = new Dictionary<string, object>();
            i++;
            Ws(s, ref i);
            if (s[i] == '}') { i++; return d; }
            while (true)
            {
                Ws(s, ref i);
                string k = Str(s, ref i);
                Ws(s, ref i);
                i++; // :
                d[k] = Value(s, ref i);
                Ws(s, ref i);
                if (s[i] == ',') { i++; continue; }
                i++; // }
                return d;
            }
        }
        if (c == '[')
        {
            var l = new List<object>();
            i++;
            Ws(s, ref i);
            if (s[i] == ']') { i++; return l; }
            while (true)
            {
                l.Add(Value(s, ref i));
                Ws(s, ref i);
                if (s[i] == ',') { i++; continue; }
                i++;
                return l;
            }
        }
        if (c == '"') return Str(s, ref i);
        if (s.Substring(i).StartsWith("true")) { i += 4; return true; }
        if (s.Substring(i).StartsWith("false")) { i += 5; return false; }
        if (s.Substring(i).StartsWith("null")) { i += 4; return null; }
        int st = i;
        while (i < s.Length && "+-0123456789.eE".IndexOf(s[i]) >= 0) i++;
        return double.Parse(s.Substring(st, i - st), CultureInfo.InvariantCulture);
    }

    static string Str(string s, ref int i)
    {
        var sb = new StringBuilder();
        i++; // opening quote
        while (s[i] != '"')
        {
            char c = s[i++];
            if (c == '\\')
            {
                char e = s[i++];
                switch (e)
                {
                    case 'n': sb.Append('\n'); break;
                    case 't': sb.Append('\t'); break;
                    case 'r': sb.Append('\r'); break;
                    case 'b': sb.Append('\b'); break;
                    case 'f': sb.Append('\f'); break;
                    case 'u': sb.Append((char)Convert.ToInt32(s.Substring(i, 4), 16)); i += 4; break;
                    default: sb.Append(e); break;
                }
            }
            else sb.Append(c);
        }
        i++;
        return sb.ToString();
    }

    public static string Write(object v)
    {
        var sb = new StringBuilder();
        W(sb, v, 0);
        return sb.ToString();
    }

    static void W(StringBuilder sb, object v, int depth)
    {
        switch (v)
        {
            case null: sb.Append("null"); return;
            case string str:
                sb.Append('"');
                foreach (char c in str)
                {
                    if (c == '"') sb.Append("\\\"");
                    else if (c == '\\') sb.Append("\\\\");
                    else if (c == '\n') sb.Append("\\n");
                    else if (c == '\r') sb.Append("\\r");
                    else if (c == '\t') sb.Append("\\t");
                    else if (c < 32 || c > 126) sb.Append("\\u").Append(((int)c).ToString("x4"));
                    else sb.Append(c);
                }
                sb.Append('"');
                return;
            case bool b: sb.Append(b ? "true" : "false"); return;
            case float f: sb.Append(Num(f)); return;
            case double d: sb.Append(Num(d)); return;
            case int n: sb.Append(n); return;
            case uint u: sb.Append(u); return;
            case long n2: sb.Append(n2); return;
            case IDictionary dict:
                sb.Append('{');
                bool first = true;
                foreach (DictionaryEntry e in dict)
                {
                    if (!first) sb.Append(',');
                    first = false;
                    sb.Append('\n').Append(' ', depth * 2 + 2);
                    W(sb, e.Key.ToString(), depth + 1);
                    sb.Append(": ");
                    W(sb, e.Value, depth + 1);
                }
                if (!first) sb.Append('\n').Append(' ', depth * 2);
                sb.Append('}');
                return;
            case IEnumerable list:
                sb.Append('[');
                bool f1 = true;
                bool scalars = true;
                foreach (var item in list) if (item is IDictionary || item is IList) { scalars = false; break; }
                foreach (var item in list)
                {
                    if (!f1) sb.Append(scalars ? ", " : ",");
                    f1 = false;
                    if (!scalars) sb.Append('\n').Append(' ', depth * 2 + 2);
                    W(sb, item, depth + 1);
                }
                if (!scalars && !f1) sb.Append('\n').Append(' ', depth * 2);
                sb.Append(']');
                return;
            default:
                sb.Append('"').Append(v.ToString()).Append('"');
                return;
        }
    }

    static string Num(double d)
    {
        if (double.IsNaN(d) || double.IsInfinity(d)) return "null";
        return Math.Round(d, 5).ToString("R", CultureInfo.InvariantCulture);
    }
}
