=== 起点 ===
  = 房间
    你好！(括号) 和 "引号" are prose.
* LIST is just choice text. # VAR is a tag
{count} + 1 is text. {count} - another thing.
{count} CONST is narrative.
{a || b} {a && b} {not a}
{a || b:yes|no}
{red|blue: darker}
{!Only once.} {&Cycle.}
{a:{b:inner|other}|outer}
* * (nested) {count > 0} [A {count} choice] -> next.room(1, true)
-> next.room(1) -> other ->
->->
->-> next.room(2)
{shuffle:
- Red: scarlet.
- Blue: sky.
}
{&
- First.
- Second.
}
{
- a:
    {b:
    Inside.
    - else:
    Outside.
    }
- else:
No.
}
~ temp x = (1 + 2) * -3
~ temp y = ()
~ temp z = (mood.happy)
~ temp q = LIST_COUNT(mood) > 0 and mood has mood.happy
~ return
