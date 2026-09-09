// Introductory comment
/* multiple
lines */
INCLUDE chapters/intro.ink
VAR courage = 2
CONST limit = 10
VAR destination = -> hall
LIST mood = calm, (happy), sad = 3
EXTERNAL notify(message)
TODO: finish the story
=== start ===
你好，月栖公寓。“Welcome,” she said (quietly).
VARIABLE is narrative, not a declaration.
A slash / and a hyphen - belong to prose.
Escaped \#tag and \{braces\} and \*stars.
Hello <> again. # speaker: alice # mood: happy
* (hello) {courage > 0} [Say hello] Hello! -> hall
+ + [Stay] -> start
- (joined) We meet again.
- - A deeper gather.
{courage} steps remain; {courage > 1:many|one}.
{red|green|blue} {&one|two} {!once|} {~heads|tails}
~ temp score = courage * 2 + 1
~ score += 1
~ courage++
~ notify("Hello {courage}")
~ temp items = (mood.calm, mood.happy)
~ temp okay = not false and courage >= -2
~ temp target = -> hall.inside
-> hall ->
<- background
->-> hall
=== hall(ref visits, -> next) ===
= inside
{ courage > 0:
You can enter.
- else:
Try later.
}
{
- courage == 1:
One.
- else:
More.
}
{shuffle stopping:
- First.
- Second.
}
=== function double(x) ===
~ return x * 2
