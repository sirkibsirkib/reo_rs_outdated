interesting cases:
A: many rules:
	a->a,b
	a->a,b
	a->a,b
	a->a,b
	a->a,b
	a->a,b
	a->a,b
	...
Weakness: checking every rule gets expensive

B: many ports:
	a->b,c,d,e,f,g,h,i,j,k,l...
Weakness: checking every time gets expensive

C: parallelism:
	a->b
	c->d
	e->f
	g->h
	{a,c,e,g} (no decomposition possible)
Weakness: design may prohibit leveraging the concurrency

D: long sequences of internal work
	a->m
	m->m
Weakness: thread gets hijacked

