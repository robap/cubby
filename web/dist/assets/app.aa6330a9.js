let __zero_modules={},__zero_cache={};function __zero_define(e,t){__zero_modules[e]=t;}function __zero_require(e){if(__zero_cache[e])return __zero_cache[e].exports;let t={exports:{}};return __zero_cache[e]=t,__zero_modules[e](t.exports,__zero_require),t.exports;}__zero_define("zero",function(exports,__zero_require){let e=[],t=null,r=new Set;function n(t){let r=e[e.length-1];r&&(t.add(r),r._sources.add(t));}function o(e){for(let t of e._sources)t.delete(e);e._sources.clear();}function a(e){let t=e,r=new Set;return{get val(){return n(r),t;},set(e){if(e!==t)for(let n of(t=e,[...r]))n._notify();},update(e){this.set(e(t));}};}function s(n){let a,s=t,l={_sources:new Set,_notify(){i();}};function i(){o(l),a&&(a(),a=void 0),e.push(l);try{let e=n();"function"==typeof e&&(a=e);}finally{e.pop();}}function u(){o(l),a&&(a(),a=void 0),s&&s._effects.delete(u),r.delete(u);}return t?t._effects.add(u):r.add(u),i(),u;}let l=function(){let e=t,r={_effects:new Set,_children:new Set,_cleanups:[],dispose(){for(let e of[...r._effects])e();for(let e of(r._effects.clear(),[...r._children]))e.dispose();for(let e of(r._children.clear(),r._cleanups))try{e();}catch(e){}r._cleanups.length=0,e&&e._children.delete(r);},onCleanup(e){r._cleanups.push(e);},run(n){let o=t;t=r,e&&e._children.add(r);try{return n();}finally{t=o;}}};return r;},i=new WeakMap,u="TEXT",c="TAG_OPEN",d="TAG_NAME",p="IN_TAG",f="ATTR_NAME",_="AFTER_ATTR_NAME",h="ATTR_VALUE_UNQUOTED",m="ATTR_VALUE_DQ",b="ATTR_VALUE_SQ",g="CLOSING_TAG",$="http://www.w3.org/2000/svg";function y(e,...t){let r=i.get(e);return r||(r=function(e){let t=document.createDocumentFragment(),r=[],n=u,o=t,a=[],s=[],l="",i="",y=!1,x=null,z=!1,w="",k="",q=0;function S(e){return q>0||"svg"===e.toLowerCase()?document.createElementNS($,e):document.createElement(e);}function T(){k&&(o.appendChild(document.createTextNode(k)),k="");}function A(){l&&(z?(x.push(i),r.push({type:"attr",path:[...s],name:l,statics:x})):o.setAttribute(l,i),l="",i="",x=null,z=!1);}for(let E=0;E<e.length;E++){let C=e[E];for(let e=0;e<C.length;e++){let r=C[e],x=C[e+1];switch(n){case u:"<"===r?(T(),"/"===x?(n=g,e++):(n=c,w="")):k+=r;break;case c:if(/[a-zA-Z]/.test(r))n=d,w=r;else throw Error(`html: unexpected char '${r}' after '<'`);break;case d:if(/[a-zA-Z0-9\-]/.test(r))w+=r;else if(" "===r||"	"===r||"\n"===r||"\r"===r){let e=S(w);e.namespaceURI===$&&q++,o.appendChild(e),a.push({el:e,pathIdx:o.childNodes.length-1,svg:e.namespaceURI===$}),s.push(o.childNodes.length-1),o=e,n=p;}else if(">"===r){let e=S(w);e.namespaceURI===$&&q++,o.appendChild(e),a.push({el:e,pathIdx:o.childNodes.length-1,svg:e.namespaceURI===$}),s.push(o.childNodes.length-1),o=e,n=u;}else if("/"===r&&">"===x){let t=S(w);o.appendChild(t),e++,n=u;}else throw Error(`html: unexpected char '${r}' in tag name`);break;case p:">"===r?n=u:"/"===r&&">"===x?(a.pop(),s.pop(),o=a.length>0?a[a.length-1].el:t,e++,n=u):" "!==r&&"	"!==r&&"\n"!==r&&"\r"!==r&&(n=f,l=r,y=!1);break;case f:"="===r?(n=_,y=!0):" "===r||"	"===r||"\n"===r||"\r"===r?n=_:">"===r?(A(),n=u):l+=r;break;case _:'"'===r?n=m:"'"===r?n=b:">"===r?(A(),n=u):"="===r?y=!0:" "===r||"	"===r||"\n"===r||"\r"===r||(y?(n=h,i=r):(A(),n=p,e--));break;case m:'"'===r?(A(),n=p):i+=r;break;case b:"'"===r?(A(),n=p):i+=r;break;case h:" "===r||"	"===r||"\n"===r||"\r"===r?(A(),n=p):">"===r?(A(),n=u):i+=r;break;case g:if(">"===r){let e=a.pop();e&&e.svg&&q--,s.pop(),o=a.length>0?a[a.length-1].el:t,n=u;}}}if(E<e.length-1)switch(T(),n){case u:{let e=document.createComment("");o.appendChild(e),r.push({type:"node",path:[...s,o.childNodes.length-1]});break;}case _:case m:case b:case h:{let e=[...s];if(l.startsWith("@")){let[t,...o]=l.slice(1).split(".");r.push({type:"event",path:e,event:t,modifiers:o}),l="",i="",n===_&&(n=p);}else"ref"===l?(r.push({type:"ref",path:e}),l="",i="",n===_&&(n=p)):(null===x&&(x=[]),x.push(i),i="",z=!0,n===_&&(n=h));break;}default:throw Error(`html: placeholder in unsupported position (state: ${n})`);}}return T(),{fragment:t,parts:r};}(e),i.set(e,r)),{_template:r,_values:t};}let x={enter:"Enter",escape:"Escape",space:" ",tab:"Tab",up:"ArrowUp",down:"ArrowDown",left:"ArrowLeft",right:"ArrowRight"};function z(e){if(null==e||"object"!=typeof e)return!1;let t=Object.getOwnPropertyDescriptor(e,"val");return!!t&&"function"==typeof t.get;}function w(e,t,r){let n=e.tagName;if("value"===t&&("INPUT"===n||"TEXTAREA"===n||"SELECT"===n)){let t=null==r?"":String(r);return e.value!==t&&(e.value=t),!0;}return"checked"===t&&"INPUT"===n?(e.checked=!!r&&"false"!==r,!0):"selected"===t&&"OPTION"===n&&(e.selected=!!r&&"false"!==r,!0);}function k(e,t,r){w(e,t,r)||(!1===r||null==r?e.removeAttribute(t):!0===r?e.setAttribute(t,""):e.setAttribute(t,String(r)));}function q(e,t,r,n){let o=r[0];for(let e=0;e<n.length;e++)o+=function e(t){return null==t?"":z(t)?e(t.val):"function"==typeof t?e(t()):String(t);}(n[e])+r[e+1];w(e,t,o)||e.setAttribute(t,o);}function S(e,t){return 0===t.currentNodes.length?e.nextSibling:t.currentNodes[t.currentNodes.length-1].nextSibling;}function T(e){for(let t of e.currentNodes)t.parentNode&&t.parentNode.removeChild(t);e.currentNodes.length=0;}function A(e){if(e.itemScopes){for(let t of e.itemScopes)t.dispose();e.itemScopes.length=0;}}function E(e,t,r){if(null==t)return;if(null!=t&&"object"==typeof t&&null!=t._template&&Array.isArray(t._values)){let n=document.createDocumentFragment();for(R(t,n);n.childNodes.length>0;){let t=n.childNodes[0];e.parentNode.insertBefore(t,S(e,r)),r.currentNodes.push(t);}return;}let n=document.createTextNode(String(t));e.parentNode.insertBefore(n,S(e,r)),r.currentNodes.push(n);}function C(e,t,r){if(T(r),null!=t){if(Array.isArray(t)){for(let n of t)E(e,n,r);return;}E(e,t,r);}}function N(e,t){for(let r of e){if(r===t)return 100;if(r.startsWith(t+":")){let e=r.slice(t.length+1);if(!/^\d+$/.test(e))throw Error(`html: invalid modifier '${r}' — expected '${t}:<ms>' with positive integer`);let n=Number(e);if(n<=0)throw Error(`html: invalid modifier '${r}' — interval must be > 0`);return n;}}return 0;}function R(e,t){let{_template:r,_values:n}=e,o=r.fragment.cloneNode(!0),a=r.parts.map(e=>(function(e,t){let r=e;for(let e of t)r=r.childNodes[e];return r;})(o,e.path)),i=0;for(let e=0;e<r.parts.length;e++){let t=r.parts[e],o=a[e];switch(t.type){case"attr":{var u,c,d;let e=t.statics.length-1;u=t.name,c=t.statics,d=n.slice(i,i+e),2===c.length&&""===c[0]&&""===c[1]?function(e,t,r){z(r)?s(()=>k(e,t,r.val)):"function"==typeof r?s(()=>k(e,t,r())):k(e,t,r);}(o,u,d[0]):function(e,t,r,n){n.some(e=>z(e)||"function"==typeof e)?s(()=>q(e,t,r,n)):q(e,t,r,n);}(o,u,c,d),i+=e;break;}case"event":!function(e,t,r,n){let o,a,l,i,u,c,d=(o=r.filter(e=>e in x),a=r.includes("prevent"),l=r.includes("stop"),i=N(r,"throttle"),u=N(r,"debounce"),c=e=>{if(!(o.length>0)||o.some(t=>e.key===x[t]))return a&&e.preventDefault?.(),l&&e.stopPropagation?.(),n(e);},i>0&&(c=function(e,t){let r=0;return(...n)=>{let o=Date.now();if(!(o-r<t))return r=o,e(...n);};}(c,i)),u>0&&(c=function(e,t){let r;return(...n)=>{clearTimeout(r),r=setTimeout(()=>e(...n),t);};}(c,u)),c),p=r.includes("once")?{once:!0}:void 0;e.addEventListener(t,d,p),s(()=>()=>e.removeEventListener(t,d,p));}(o,t.event,t.modifiers,n[i]),i++;break;case"ref":!function(e,t){t.el=e,s(()=>()=>{t.el=null;});}(o,n[i]),i++;break;case"node":!function(e,t,r){if(A(r),T(r),null!=t){if(t&&t._isEach)return function(e,t,r){if("function"==typeof t.keyFn)return function(e,t,r){let{signal:n,renderFn:o,keyFn:a}=t;r.itemsByKey=r.itemsByKey||Object.create(null),s(()=>{let t=n.val;if(!Array.isArray(t)){for(let e in r.itemsByKey)r.itemsByKey[e].scope.dispose();r.itemsByKey=Object.create(null),T(r);return;}let s=Array(t.length),i=Object.create(null);for(let e=0;e<t.length;e++){let r=String(a(t[e],e));if(i[r])throw Error(`each: duplicate key '${r}' in row ${e}`);i[r]=!0,s[e]=r;}let u=r.itemsByKey,c=Object.create(null),d=e.parentNode;for(let e in u)if(!i[e]){let t=u[e];for(let e of(t.scope.dispose(),t.nodes))e.parentNode&&e.parentNode.removeChild(e);}let p=[],f=e.nextSibling;for(let e=0;e<t.length;e++){let r=s[e],n=u[r];if(null==n){let r=l(),a=[];r.run(()=>{let r=o(t[e],e),n=document.createDocumentFragment();for(R(r,n);n.childNodes.length>0;){let e=n.childNodes[0];d.insertBefore(e,f),a.push(e),p.push(e);}}),n={scope:r,nodes:a};}else for(let e of n.nodes)e!==f?d.insertBefore(e,f):f=e.nextSibling,p.push(e);c[r]=n,n.nodes.length>0&&(f=n.nodes[n.nodes.length-1].nextSibling);}r.itemsByKey=c,r.currentNodes=p;});}(e,t,r);let{signal:n,renderFn:o}=t;r.itemScopes=r.itemScopes||[],s(()=>{A(r),T(r);let t=n.val;if(Array.isArray(t))for(let n=0;n<t.length;n++){let a=l();r.itemScopes.push(a),a.run(()=>{let a=o(t[n],n),s=document.createDocumentFragment();for(R(a,s);s.childNodes.length>0;){let t=s.childNodes[0];e.parentNode.insertBefore(t,S(e,r)),r.currentNodes.push(t);}});}});}(e,t,r);if(z(t))return s(()=>C(e,t.val,r));if("function"==typeof t)return s(()=>C(e,t(),r));C(e,t,r);}}(o,n[i],{currentNodes:[]}),i++;}}t.appendChild(o);}function L(e){return"/"===e?e:e.endsWith("/")?e.slice(0,-1):e;}function j(e){if(!e||"?"===e)return{};let t=e.startsWith("?")?e.slice(1):e,r={};for(let e of t.split("&")){let t=e.indexOf("=");-1===t?r[decodeURIComponent(e)]="":r[decodeURIComponent(e.slice(0,t))]=decodeURIComponent(e.slice(t+1));}return r;}function O(e){let t=e.indexOf("#"),r=t>=0?e.slice(0,t):e,n=r.indexOf("?");return n>=0?{pathname:r.slice(0,n),search:r.slice(n)}:{pathname:r,search:""};}function D(e,t){let{pathname:r,search:n}=O(t),o=L(r),a=j(n);for(let t of e){let e=function(e,t){let r=e.regex.exec(t);if(!r)return null;let n={};for(let t=0;t<e.paramNames.length;t++)n[e.paramNames[t]]=decodeURIComponent(r[t+1]);return{params:n};}(t.compiled,o);if(e)return{route:t,params:e.params,query:a,pathname:o,search:n};}return null;}let B=null;function M({pattern:e,normalized:t,loaderOrLoad:r,opts:n}){return{pattern:e,normalized:t,loaderOrLoad:r,opts:n,resolvedComponent:null};}exports.signal=a,exports.computed=function(t){let r={_value:void 0,_dirty:!0,_subscribers:new Set},a={_sources:new Set,_notify(){if(!r._dirty)for(let e of(r._dirty=!0,[...r._subscribers]))e._notify();},get val(){return r._dirty&&function(t,r,n){o(t),e.push(t);try{r._value=n();}finally{e.pop();}r._dirty=!1;}(a,r,t),n(r._subscribers),r._value;}};return a;},exports.effect=s,exports.html=y,exports.commit=R,exports.each=function(e,t,r){return{_isEach:!0,signal:e,renderFn:t,keyFn:r};},exports.ref=function(){return{el:null};},exports.App=class{constructor(){this._state=new Map,this._routes=[],this._layout=null,this._pathSig=a(""),this._paramsSig=a({}),this._querySig=a({}),this._mountEl=null,this._running=!1,this._rootSlotSig=a(null),this._rootScope=null,this._stateProxy=new Proxy({},{get:(e,t)=>this._state.get(t)}),this._middleware=[],this._navToken=0,this._loading=null,this._error=null,this._navScope=null,this._chain=[],this._lastCommittedUrl=null;}_computeDivergence(e,t){let r=0;for(;r<e.length&&r<t.length&&e[r].descriptor===t[r];)r++;return r;}_resolveLoadingFor(e,t){for(let r=t;r<e.length;r++)if(e[r].opts.loading)return e[r].opts.loading;return this._loading;}_mergeMeta(e){return e.reduce((e,t)=>Object.assign({},e,t.opts.meta||{}),{});}_slotAt(e){return 0===e?this._rootSlotSig:this._chain[e-1].outletSig;}_assertNotRunning(e){if(this._running)throw Error(`App.${e}() cannot be called after run()`);}state(e,t){if(this._assertNotRunning("state"),this._state.has(e))throw Error(`App.state: key "${e}" already registered`);return this._state.set(e,t),this;}layout(e){if(this._assertNotRunning("layout"),null!=this._layout)throw Error("App.layout: layout already set");if("function"!=typeof e)throw Error("App.layout: component must be a function");return this._layout=e,this;}use(e){if(this._assertNotRunning("use"),"function"!=typeof e)throw Error("App.use: middleware must be a function");return this._middleware.push(e),this;}loading(e){if(this._assertNotRunning("loading"),null!=this._loading)throw Error("App.loading: loading already set");if("function"!=typeof e)throw Error("App.loading: component must be a function");return this._loading=e,this;}error(e){if(this._assertNotRunning("error"),null!=this._error)throw Error("App.error: error already set");if("function"!=typeof e)throw Error("App.error: component must be a function");return this._error=e,this;}route(e,t,r={}){if(this._assertNotRunning("route"),"function"!=typeof t)throw Error("App.route: handler must be a function");if(null!=r.children&&!Array.isArray(r.children))throw Error("App.route: opts.children must be an array");if(null!=r.guard&&"function"!=typeof r.guard)throw Error("App.route: guard must be a function");if(null!=r.load&&"function"!=typeof r.load)throw Error("App.route: load must be a function");if(null!=r.meta&&("object"!=typeof r.meta||Array.isArray(r.meta)))throw Error("App.route: meta must be an object");if(null!=r.loading&&"function"!=typeof r.loading)throw Error("App.route: loading must be a function");if(null!=r.error&&"function"!=typeof r.error)throw Error("App.route: error must be a function");let n=L(e),{children:o,...a}=r,s=M({pattern:e,normalized:n,loaderOrLoad:t,opts:a});return this._flattenRoutes(s,[s],o),this;}_flattenRoutes(e,t,r){if(!r||0===r.length){let{normalized:r}=e;this._routes.push({pattern:e.pattern,normalized:r,compiled:function(e){if("*"===e)return{pattern:e,normalized:"*",paramNames:[],regex:/^.*$/,isWildcard:!0};let t=L(e),r=[],n=RegExp("^"+t.split("/").map(e=>e.startsWith(":")?(r.push(e.slice(1)),"([^/]+)"):e.replace(/[.*+?^${}()|[\]\\]/g,"\\$&")).join("\\/")+"$");return{pattern:e,normalized:t,paramNames:r,regex:n,isWildcard:!1};}(r),loader:e.loaderOrLoad,opts:e.opts,resolvedComponent:null,chain:t});return;}for(let n of r){if("function"!=typeof n.load)throw Error("App.route: each child entry must have a load function");let{children:r,...o}=n,a=function(e,t){let r=L(e);return"/"===t?r:"/"===r?L(t):L(r+t);}(e.normalized,n.path),s=M({pattern:n.path,normalized:a,loaderOrLoad:n.load,opts:o});this._flattenRoutes(s,[...t,s],r);}}match(e){return D(this._routes,e);}run(e){if(this._running)throw Error("App.run: already running");let t=document.querySelector(e);if(!t)throw Error(`App.run: element not found for selector "${e}"`);this._mountEl=t,this._running=!0,B=this,this._rootScope=l(),this._rootScope.run(()=>{this._layout?R(this._layout({outlet:this._rootSlotSig}),this._mountEl):R(y`${this._rootSlotSig}`,this._mountEl);});let r=window.location.pathname+window.location.search;this._navigateTo(r);let n=()=>this._navigateTo(window.location.pathname+window.location.search);this._popstateListener=n,window.addEventListener("popstate",n),this._rootScope.onCleanup(()=>window.removeEventListener("popstate",n));let o=e=>(function(e){var t;let r;if(e.defaultPrevented||null!=e.button&&0!==e.button||e.metaKey||e.ctrlKey||e.shiftKey||e.altKey)return;let n=e.target;for(;n&&"A"!==n.tagName;)n=n.parentNode;if(!n)return;let o=n.getAttribute("target");if(o&&"_self"!==o||n.hasAttribute("download")||n.hasAttribute("data-external"))return;let a=n.getAttribute("href");!(!a||a.startsWith("#")||/^[a-z][a-z0-9+\-.]*:/i.test(a)&&!a.startsWith(window.location.origin))&&(e.preventDefault(),t=a.startsWith(window.location.origin)?a.slice(window.location.origin.length):a,(r=B)&&(window.history.pushState(null,"",t),r._navigateTo(t)));})(e);this._clickListener=o,document.addEventListener("click",o),this._rootScope.onCleanup(()=>document.removeEventListener("click",o));}_navigateTo(e){var t;let r=++this._navToken,n=D(this._routes,e);if(n)this._pathSig.set(n.pathname),this._paramsSig.set(n.params),this._querySig.set(n.query);else{let{pathname:t,search:r}=O(e);this._pathSig.set(L(t)),this._paramsSig.set({}),this._querySig.set(j(r));}if(null==n)return void this._rootSlotSig.set(null);this._navScope&&(this._navScope.dispose(),this._navScope=null),this._navScope=l();let o=new AbortController;this._navScope.onCleanup(()=>o.abort());let s=(t=o.signal,(e,r={})=>{let n=r.signal,o=n?function(e,t){if("u">typeof AbortSignal&&"function"==typeof AbortSignal.any)return AbortSignal.any([e,t]);let r=new AbortController,n=e=>()=>{r.abort(e.reason);};return e.aborted?r.abort(e.reason):e.addEventListener("abort",n(e)),t.aborted?r.abort(t.reason):t.addEventListener("abort",n(t)),r.signal;}(t,n):t;return globalThis.fetch(e,{...r,signal:o});}),i=this;(async()=>{let t=i._stateProxy,u=n.route.chain,c=u.length,d=i._computeDivergence(i._chain,u);d=Math.min(d,c-1);let p=i._slotAt(d),f=i._mergeMeta(u),_={path:n.pathname,params:n.params,query:n.query,meta:f},h=i._resolveLoadingFor(u,d),m=setTimeout(()=>{r!==i._navToken||h&&i._navScope.run(()=>{p.set(h());});},150);try{let e;for(let e of i._middleware){let n=!1,o=(e,t={})=>{n=!0,i._navToken++,window.history.replaceState(null,"",e),i._navigateTo(e);};if(await e({route:_,state:t,redirect:o}),r!==i._navToken||n)return void clearTimeout(m);}for(let e=d;e<c;e++){let o=u[e];if(o.opts.guard){let e=(e,t={})=>{i._navToken++,window.history.replaceState(null,"",e),i._navigateTo(e);},a=await o.opts.guard({params:n.params,query:n.query,state:t,route:_,redirect:e});if(r!==i._navToken)return void clearTimeout(m);if(!1===a){clearTimeout(m),null!=i._lastCommittedUrl&&window.history.replaceState(null,"",i._lastCommittedUrl);return;}}if(o.opts.load&&(await o.opts.load({params:n.params,query:n.query,state:t,fetch:s,route:_}),r!==i._navToken))return void clearTimeout(m);}clearTimeout(m);for(let e=i._chain.length-1;e>=d;e--)i._chain[e].scope.dispose();i._chain.length=d;let o=[];for(let s=c-1;s>=d;s--){let d,p=u[s],f=s===c-1?null:a(e),_=l();if(null==p.resolvedComponent){let e=p.loaderOrLoad({params:n.params,query:n.query,state:t,...null!=f?{outlet:f}:{}});if(null!=e&&"function"==typeof e.then){let o=await e;if(r!==i._navToken)return void clearTimeout(m);p.resolvedComponent=o.default,_.run(()=>{d=p.resolvedComponent({params:n.params,query:n.query,state:t,...null!=f?{outlet:f}:{}});});}else p.resolvedComponent=p.loaderOrLoad,_.run(()=>{d=e;});}else _.run(()=>{d=p.resolvedComponent({params:n.params,query:n.query,state:t,...null!=f?{outlet:f}:{}});});o.unshift({descriptor:p,scope:_,outletSig:f}),e=d;}for(let e of o)i._chain.push(e);i._chain[d].scope.run(()=>{p.set(e);}),i._lastCommittedUrl=n.pathname+n.search,function(e,t,r){for(let n of e.querySelectorAll("a")){let e,o,a=n.getAttribute("href");if(!a||a.startsWith("#")){n.removeAttribute("data-active"),n.removeAttribute("data-active-exact");continue;}if(a.startsWith("/")){let t=a.indexOf("?");t>=0?(e=a.slice(0,t),o=a.slice(t)):(e=a,o="");}else if(a.startsWith(window.location.origin)){let t=a.slice(window.location.origin.length),r=t.indexOf("?");r>=0?(e=t.slice(0,r),o=t.slice(r)):(e=t,o="");}else{n.removeAttribute("data-active"),n.removeAttribute("data-active-exact");continue;}let s=(e=L(e))===t&&o===r,l=t===e||t.startsWith(e+"/");s?(n.setAttribute("data-active-exact",""),n.setAttribute("data-active","")):(l?n.setAttribute("data-active",""):n.removeAttribute("data-active"),n.removeAttribute("data-active-exact"));}}(i._mountEl,n.pathname,n.search);}catch(t){if(r!==i._navToken)return;if(t&&"AbortError"===t.name&&o.signal.aborted)return void clearTimeout(m);if(clearTimeout(m),i._error){i._navScope.dispose(),i._navScope=l();let r=()=>i._navigateTo(e);i._navScope.run(()=>{p.set(i._error({error:t,retry:r}));}),i._chain[d]={descriptor:null,scope:i._navScope,outletSig:null},i._chain.length=d+1;}else console.error("navigation error",t);}})();}_getState(e){if(!this._state.has(e))throw Error(`inject: key "${e}" is not registered`);return this._state.get(e);}},exports.inject=function(e){if(null==B)throw Error("inject: no app is running");return B._getState(e);},exports.navigate=function(e,t={}){let r=B;if(!r)throw Error("navigate: no app is running");let n=t.state??null;t.replace?window.history.replaceState(n,"",e):window.history.pushState(n,"",e),r._navigateTo(e);},exports.back=function(){if(!B)throw Error("back: no app is running");window.history.back();},exports.forward=function(){if(!B)throw Error("forward: no app is running");window.history.forward();},exports.route=function(){let e=B;if(!e)throw Error("route: no app is running");return{get path(){return e._pathSig.val;},get params(){return e._paramsSig.val;},get query(){return e._querySig.val;}};},exports._setCurrentApp=function(e){B=e;},exports._createScope=l,exports._getCurrentApp=function(){return B;},exports._disposeUnownedEffects=function(){for(let e of[...r])e();r.clear();};}),__zero_define("zero/http",function(exports,__zero_require){class e extends Error{constructor(e,t,r){super(`HTTP ${e} ${t}`),this.name="HttpError",this.status=e,this.statusText=t,this.body=r;}}async function t(e,t,n){let o=e=>async r=>e>=t.length?n(r):t[e](r,o(e+1));return r(await o(0)(e));}async function r(e){let t=e.headers.get("Content-Type")||"",r=/\bjson\b/i.test(t),a=""===t;if(!e.ok)return n(e,r,a);if(r)return e.json();if(a){let{parsed:t,value:r}=await o(e);return t?r:e;}return e;}async function n(t,r,n){let a;if(r)try{a=await t.json();}catch(e){a=void 0;}else if(n){let{parsed:e,value:r,text:n}=await o(t);a=e?r:n;}else try{a=await t.text();}catch(e){a=void 0;}throw new e(t.status,t.statusText,a);}async function o(e){let t;try{t=await e.text();}catch(e){return{parsed:!1,value:void 0,text:""};}try{return{parsed:!0,value:JSON.parse(t),text:t};}catch(e){return{parsed:!1,value:void 0,text:t};}}exports.createHttp=function(e={}){let r=e.fetch??globalThis.fetch,n=[];function o(e,o,a,s){return function(e,r,n,o,a,s){let{fetch:l,...i}=o??{},u={...i,method:e},c=new Headers(u.headers||{});return void 0!==n&&(function(e){if(null===e||"object"!=typeof e||"u">typeof FormData&&e instanceof FormData||"u">typeof Blob&&e instanceof Blob||e instanceof ArrayBuffer||ArrayBuffer.isView(e)||"u">typeof URLSearchParams&&e instanceof URLSearchParams||"u">typeof ReadableStream&&e instanceof ReadableStream)return!1;let t=Object.getPrototypeOf(e);return t===Object.prototype||null===t;}(n)||Array.isArray(n)?(c.has("Content-Type")||c.set("Content-Type","application/json"),u.body=JSON.stringify(n)):u.body=n),u.headers=c,t(new Request(r,u),a,l??s);}(e,o,a,s,n,r);}let a={use(e){if("function"!=typeof e)throw TypeError("HttpClient.use: middleware must be a function");return n.push(e),a;},get:(e,t)=>o("GET",e,void 0,t),post:(e,t,r)=>o("POST",e,t,r),put:(e,t,r)=>o("PUT",e,t,r),patch:(e,t,r)=>o("PATCH",e,t,r),delete:(e,t)=>o("DELETE",e,void 0,t),request:(e,o)=>(function(e,r,n,o){let{fetch:a,...s}=r??{};return t(e instanceof Request&&0===Object.keys(s).length?e:new Request(e,s),n,a??o);})(e,o,n,r)};return a;},exports.HttpError=e;}),__zero_define("./src/app.ts",function(exports,__zero_require){let{App:e}=__zero_require("zero"),t=__zero_require("./src/components/chrome.ts").default,r=__zero_require("./src/routes/live-log.ts").default,{default:n,load:o}=__zero_require("./src/routes/browser.ts"),{applyTheme:a,loadHealth:s}=__zero_require("./src/stores/chrome.ts");a(),s(),new e().layout(t).route("/_/",r).route("/_",r).route("/_/browser",n,{load:o}).route("*",r).run("#app");}),__zero_define("./src/stores/chrome.ts",function(exports,__zero_require){let e,{signal:t}=__zero_require("zero"),{getHealth:r}=__zero_require("./src/lib/api.ts"),n=t(null),o=t(!1);async function a(){try{let e=await r();n.set(e),o.set("ok"===e.status);}catch{o.set(!1);}}let s=t("light"===(e=document.documentElement.getAttribute("data-theme"))||"dark"===e?e:"function"==typeof matchMedia&&matchMedia("(prefers-color-scheme: light)").matches?"light":"dark");exports.loadHealth=a,exports.toggleTheme=function(){let e="dark"===s.val?"light":"dark";s.set(e),document.documentElement.setAttribute("data-theme",e);},exports.applyTheme=function(){document.documentElement.setAttribute("data-theme",s.val);},exports.health=n,exports.healthy=o,exports.theme=s;}),__zero_define("./src/lib/api.ts",function(exports,__zero_require){let{createHttp:e}=__zero_require("zero/http"),t=e({fetch:(...e)=>globalThis.fetch(...e)});function r(e){return e.split("/").map(encodeURIComponent).join("/");}async function n(e,t,n){let o=await fetch(`/_/api/buckets/${encodeURIComponent(e)}/objects/${r(t)}`,{method:"PUT",body:n});if(!o.ok)throw Error(`upload failed: ${o.status}`);}exports.getHealth=function(){return t.get("/_/api/health");},exports.listBuckets=function(){return t.get("/_/api/buckets");},exports.createBucket=function(e){return t.post("/_/api/buckets",{name:e});},exports.listObjects=function(e,r,n){let o=new URLSearchParams({delimiter:"/",prefix:r});return n&&o.set("continuation-token",n),t.get(`/_/api/buckets/${encodeURIComponent(e)}/objects?${o}`);},exports.search=function(e,r){let n=new URLSearchParams({q:e});return r&&n.set("bucket",r),t.get(`/_/api/search?${n}`);},exports.getMeta=function(e,n){return t.get(`/_/api/buckets/${encodeURIComponent(e)}/objects/${r(n)}`);},exports.contentUrl=function(e,t){return`/_/api/buckets/${encodeURIComponent(e)}/objects/${r(t)}?content`;},exports.uploadObject=n,exports.deleteObject=function(e,n){return t.delete(`/_/api/buckets/${encodeURIComponent(e)}/objects/${r(n)}`);},exports.presign=function(e){return t.post("/_/api/presign",e);};}),__zero_define("./src/routes/browser.ts",function(exports,__zero_require){let{each:e,effect:t,html:r,signal:n}=__zero_require("zero"),{Input:o}=__zero_require("./.zero/components/index.ts"),{HttpError:a}=__zero_require("zero/http"),{contentUrl:s}=__zero_require("./src/lib/api.ts"),{crumbs:l,folderLabel:i,highlightParts:u,viewMode:c}=__zero_require("./src/lib/browse.ts"),{baseName:d,fmtDate:p,humanBytes:f}=__zero_require("./src/lib/format.ts"),_=__zero_require("./src/components/object-detail.ts").default,{allBuckets:h,buckets:m,createBucket:b,folder:g,loadBuckets:$,navigateTo:y,openObject:x,prefix:z,removeObject:w,searchResults:k,searchTerm:q,selectBucket:S,selectedBucket:T,selectedObject:A,setSearch:E,toggleAllBuckets:C,uploadFiles:N}=__zero_require("./src/stores/browse.ts");exports.default=function(){return r`${()=>{let $,R,L,j,O,D;return A.val?_():r`
    <section class="screen browser-screen flank gap-0">
      ${r`
    <div class="buckets-col stack">
      <div class="section-label pad-sm">BUCKETS</div>
      ${e(m,e=>{let t=e.object_count>0?f(e.size):"—";return r`
      <button class=${()=>"bucket-row flex-col text-start"+(T.val===e.name?" active":"")} @click=${()=>S(e.name)}>
        <span class="bucket-name mono">${e.name}</span>
        <span class="bucket-sub mono muted">${e.object_count} objects · ${t}</span>
      </button>
    `;},e=>e.name)}
      ${$=n(!1),R=n(""),L=n(null),j=async()=>{let e=R.val.trim();if(e)try{await b(e),R.set(""),L.set(null),$.set(!1);}catch(e){L.set(function(e){if(e instanceof a){let t=e.body;return t?.error?.message??`Request failed (${e.status})`;}return"Could not create bucket.";}(e));}},r`
    <div class="new-bucket stack gap-xs pad-sm">
      ${()=>$.val?r`
              <form class="cluster align-center gap-sm" @submit=${e=>{e.preventDefault(),j();}}>
                ${o({value:R,placeholder:"bucket-name",size:"sm",autofocus:!0,error:L})}
                <button class="button button-primary button-sm" type="button" @click=${j}>Create</button>
              </form>
            `:r`<button class="new-bucket-btn" @click=${()=>$.set(!0)}>+ New bucket</button>`}
    </div>
  `}
    </div>
  `}
      ${O=n(!1),r`
    <div
      class=${()=>"listing-pane flex-col"+(O.val?" dragging":"")}
      @drop=${e=>{e.preventDefault(),O.set(!1);let t=e.dataTransfer?.files;t&&t.length>0&&N(Array.from(t));}}
      @dragover=${e=>{e.preventDefault(),O.set(!0);}}
      @dragleave=${()=>O.set(!1)}
    >
      ${D=n(""),t(()=>D.set(q.val)),r`
    <div class="listing-toolbar split align-center pad-md border-b">
      <div class="cluster align-center gap-md">
        <div class="search-field">
          ${o({value:D,placeholder:"Search keys…",size:"sm",onChange:e=>E(e),debounceMs:150})}
        </div>
        <button class=${()=>"all-buckets-btn"+(h.val?" active":"")} @click=${C}>all buckets</button>
      </div>
      <span class="mono muted">
        ${()=>{let e=k.val;return e?`${e.results.length} matches`:"";}}
      </span>
    </div>
  `}
      ${()=>"search"===c(q.val)?r`
    <div class="search-results">
      ${()=>{let e=k.val;if(!e)return r`<div class="pad-lg muted">Searching…</div>`;if(0===e.results.length){let e=q.val;return r`<div class="pad-lg muted">No keys match “${e}”.</div>`;}return r`<table class="listing-table search-table"><tbody>${e.results.map(e=>{var t;let n;return n=u((t=e).key,q.val),r`
    <tr class="listing-row search-row" @click=${()=>x(t.bucket,t.key)}>
      <td class="c-name">
        <span class="cluster align-center gap-sm">
          ${h.val?r`<span class="bucket-tag mono">${t.bucket}</span>`:""}
          <span class="mono">${n.map(e=>e.match?r`<mark>${e.text}</mark>`:e.text)}</span>
        </span>
      </td>
      <td class="c-size mono">${f(t.size)}</td>
      <td class="c-mod mono muted">${p(t.last_modified)}</td>
    </tr>
  `;})}</tbody></table>`;}}
    </div>
  `:r`
    <div class="folder-view">
      ${r`
    <div class="breadcrumb cluster align-center gap-xs pad-md">
      ${()=>{let e=T.val;if(!e)return"";let t=l(e,z.val);return r`${t.map((e,t)=>r`${t>0?r`<span class="crumb-sep muted">/</span>`:""}<button
            class="crumb mono"
            @click=${()=>y(e.prefix)}
          >${e.label}</button>`)}`;}}
    </div>
  `}
      ${()=>{var e,t;let n,o=g.val;return o?0===o.common_prefixes.length&&0===o.objects.length?r`
    <div class="empty-state text-center stack gap-sm align-center justify-center">
      <div class="empty-icon" aria-hidden="true">🗃️</div>
      <div>No objects yet.</div>
      <div class="muted">Drop files to upload to <span class="mono">${()=>`${T.val??""}/${z.val}`}</span></div>
    </div>
  `:(e=o.common_prefixes,t=o.objects,n=z.val,r`
    <table class="listing-table">
      <thead>
        <tr><th class="c-name text-start">NAME</th><th class="c-size text-start">SIZE</th><th class="c-mod text-start">MODIFIED</th><th class="c-etag text-start">ETAG</th></tr>
      </thead>
      <tbody>
        ${e.map(e=>{var t,o;return t=i(e,n),o=e,r`
    <tr class="listing-row folder-row" @click=${()=>y(o)}>
      <td class="c-name"><span class="cluster align-center gap-sm"><span class="folder-icon" aria-hidden="true">📁</span><span class="mono">${t}</span></span></td>
      <td class="c-size mono muted">—</td>
      <td class="c-mod mono muted">—</td>
      <td class="c-etag mono muted">—</td>
    </tr>
  `;})}
        ${t.map(e=>{var t;let n;return t=e,n=T.val,r`
    <tr class="listing-row object-row">
      <td class="c-name" @click=${()=>x(n,t.key)}>
        <span class="cluster align-center gap-sm"><span class="file-icon" aria-hidden="true">📄</span><span class="mono link">${d(t.key)}</span></span>
      </td>
      <td class="c-size mono">${f(t.size)}</td>
      <td class="c-mod mono muted">${p(t.last_modified)}</td>
      <td class="c-etag mono muted">
        <span class="cluster align-center gap-sm">
          <span class="etag-val">${t.etag}</span>
          <a class="row-action row-download" href=${s(n,t.key)} download title="Download">↓</a>
          <button class="row-action row-delete" @click=${()=>w(t.key)} title="Delete">✕</button>
        </span>
      </td>
    </tr>
  `;})}
      </tbody>
    </table>
  `):r`<div class="pad-lg muted">Loading…</div>`;}}
    </div>
  `}
      <div class="drop-overlay align-center justify-center"><span>Drop to upload to ${()=>`${T.val??""}/${z.val}`}</span></div>
    </div>
  `}
    </section>
  `;}}`;},exports.load=function(){return $();};}),__zero_define("./src/stores/browse.ts",function(exports,__zero_require){let{signal:e}=__zero_require("zero"),{createBucket:t,deleteObject:r,getMeta:n,listBuckets:o,listObjects:a,presign:s,search:l,uploadObject:i}=__zero_require("./src/lib/api.ts"),{uploadKey:u}=__zero_require("./src/lib/browse.ts"),{loadHealth:c}=__zero_require("./src/stores/chrome.ts"),d=e([]),p=e(null),f=e(""),_=e(null),h=e(""),m=e(!1),b=e(null),g=e(null),$=e(null),y=e(null);async function x(){let e=await o();d.set(e.buckets),null===p.val&&e.buckets.length>0&&await w(e.buckets[0].name);}async function z(e){await t(e),await x(),await w(e),await c();}async function w(e){p.set(e),f.set(""),h.set(""),b.set(null),g.set(null),await q();}async function k(e){f.set(e),g.set(null),await q();}async function q(){let e=p.val;e&&_.set(await a(e,f.val));}async function S(e){(h.set(e),0===e.trim().length)?b.set(null):await A();}async function T(){m.set(!m.val),h.val.trim().length>0&&await A();}async function A(){let e=m.val?null:p.val;b.set(await l(h.val,e));}async function E(e,t){g.set(t),$.set(null),y.set(null),$.set(await n(e,t));}async function C(e){let t=p.val;if(t){for(let r of e)await i(t,u(f.val,r.name),r);await q(),await x(),await c();}}async function N(e){let t=p.val;t&&(await r(t,e),await q(),await x(),await c());}async function R(e,t){let r=p.val,n=g.val;if(!r||!n)return;let o=await s({method:e,bucket:r,key:n,expires_in_s:t});y.set(o.url);}exports.loadBuckets=x,exports.createBucket=z,exports.selectBucket=w,exports.navigateTo=k,exports.loadFolder=q,exports.setSearch=S,exports.toggleAllBuckets=T,exports.runSearch=A,exports.openObject=E,exports.closeObject=function(){g.set(null),$.set(null),y.set(null);},exports.uploadFiles=C,exports.removeObject=N,exports.generatePresign=R,exports.buckets=d,exports.selectedBucket=p,exports.prefix=f,exports.folder=_,exports.searchTerm=h,exports.allBuckets=m,exports.searchResults=b,exports.selectedObject=g,exports.objectMeta=$,exports.presignedUrl=y;}),__zero_define("./src/lib/browse.ts",function(exports,__zero_require){exports.viewMode=function(e){return e.trim().length>0?"search":"folder";},exports.crumbs=function(e,t){let r=[{label:e,prefix:""}],n=t.split("/").filter(e=>e.length>0),o="";for(let e of n)o+=`${e}/`,r.push({label:e,prefix:o});return r;},exports.folderLabel=function(e,t){return e.startsWith(t)?e.slice(t.length):e;},exports.uploadKey=function(e,t){return`${e}${t}`;},exports.highlightParts=function(e,t){if(!t)return[{text:e,match:!1}];let r=t.toLowerCase(),n=e.toLowerCase(),o=[],a=0,s=n.indexOf(r,a);for(;-1!==s;)s>a&&o.push({text:e.slice(a,s),match:!1}),o.push({text:e.slice(s,s+r.length),match:!0}),a=s+r.length,s=n.indexOf(r,a);return a<e.length&&o.push({text:e.slice(a),match:!1}),o.length>0?o:[{text:e,match:!1}];};}),__zero_define("./src/components/object-detail.ts",function(exports,__zero_require){let{effect:e,html:t,signal:r}=__zero_require("zero"),{Button:n,Select:o}=__zero_require("./.zero/components/index.ts"),{contentUrl:a}=__zero_require("./src/lib/api.ts"),{fmtDate:s,groupDigits:l,humanBytes:i}=__zero_require("./src/lib/format.ts"),{EXPIRY_OPTIONS:u,previewKind:c}=__zero_require("./src/lib/preview.ts"),{closeObject:d,generatePresign:p,objectMeta:f,prefix:_,presignedUrl:h,selectedBucket:m,selectedObject:b}=__zero_require("./src/stores/browse.ts");function g(e){e.target.select();}exports.default=function(){var $;let y,x,z,w,k=r(null);return $=k,e(()=>{let e=f.val,t=m.val,r=b.val;if($.set(null),!e||!t||!r)return;let n=c(e.content_type,e.size);("text"===n||"json"===n)&&fetch(a(t,r)).then(e=>e.text()).then(e=>$.set(e)).catch(()=>$.set("(failed to load preview)"));}),t`
    <section class="screen detail-screen flex-col">
      <header class="detail-topbar split align-center pad-md border-b">
        <button class="crumb-back cluster align-center gap-xs" @click=${d}>
          <span aria-hidden="true">‹</span>
          <span class="mono">${()=>`${m.val??""}/${_.val}`}</span>
        </button>
        <div class="cluster align-center gap-sm preview-label">
          <span class="chrome-label">PREVIEW</span>
          <span class="mono">${()=>f.val?.content_type??"—"}</span>
        </div>
      </header>
      <div class="detail-body">
        <div class="preview-pane flex-row align-center justify-center">${()=>(function(e,r){let n=m.val,o=b.val;if(!e||!n||!o)return t`<div class="preview-empty muted">Loading…</div>`;let s=c(e.content_type,e.size);return"image"===s?t`<img class="preview-img" src=${a(n,o)} alt=${o} />`:"text"===s||"json"===s?t`<pre class="preview-text mono">${r??"Loading…"}</pre>`:t`
    <div class="preview-download stack gap-md align-center justify-center">
      <div class="muted">No inline preview for <span class="mono">${e.content_type??"this type"}</span>.</div>
      <a class="button button-secondary button-md" href=${a(n,o)} download>Download</a>
    </div>
  `;})(f.val,k.val)}</div>
        <aside class="meta-pane stack gap-lg pad-lg">
          ${()=>{var e;let r,n;return f.val?(r=Object.entries((e=f.val).metadata??{}),n=(e,r)=>t`<div class="meta-row"><span class="meta-k">${e}</span><span class="meta-v mono">${r}</span></div>`,t`
    <div class="stack gap-md">
      <div>
        <div class="section-label">OBJECT</div>
        <div class="meta-table">
          ${n("size",`${i(e.size)} (${l(e.size)} bytes)`)}
          ${n("content-type",e.content_type??"—")}
          ${n("etag",e.etag)}
          ${n("last-modified",`${s(e.last_modified)} UTC`)}
          ${n("storage-class",e.storage_class)}
        </div>
      </div>
      ${r.length>0?t`
            <div>
              <div class="section-label">USER METADATA</div>
              <div class="meta-table">
                ${r.map(([e,r])=>t`<div class="meta-row"><span class="meta-k mono accent">x-amz-meta-${e}</span><span class="meta-v mono">${r}</span></div>`)}
              </div>
            </div>
          `:""}
    </div>
  `):t`<div class="muted">Loading…</div>`;}}
          ${y=r("GET"),x=r(String(u[1].seconds)),z=u.map(e=>({value:String(e.seconds),label:e.label})),w=e=>t`
    <button
      class=${()=>"seg-btn"+(y.val===e?" active":"")}
      @click=${()=>y.set(e)}
    >${e}</button>
  `,t`
    <div class="presign-card border pad-lg stack gap-md">
      <div>
        <div class="text-h4">Generate presigned URL</div>
        <div class="muted">Time-limited link, no credentials required.</div>
      </div>
      <div class="cluster gap-lg">
        <div class="stack gap-xs">
          <span class="chrome-label">METHOD</span>
          <div class="segmented cluster">${w("GET")}${w("PUT")}</div>
        </div>
        <div class="stack gap-xs presign-expiry">
          <span class="chrome-label">EXPIRES IN</span>
          ${o({value:x,options:z,size:"sm"})}
        </div>
      </div>
      ${n({variant:"primary",children:"Generate URL",onClick:()=>p(y.val,Number(x.val))})}
      <input
        class="presign-url mono"
        readonly
        value=${h}
        hidden=${()=>!h.val}
        @focus=${g}
      />
    </div>
  `}
        </aside>
      </div>
    </section>
  `;};}),__zero_define("./src/lib/preview.ts",function(exports,__zero_require){let e=new Set(["application/xml","application/javascript","application/x-javascript","application/x-sh"]);exports.previewKind=function(t,r){let n=(t??"").toLowerCase().split(";")[0]?.trim()??"";return n.startsWith("image/")?"image":!("application/json"===n||n.endsWith("+json")||n.startsWith("text/")||e.has(n))||r>2097152?"none":"application/json"===n||n.endsWith("+json")?"json":"text";},exports.PREVIEW_MAX_BYTES=2097152,exports.EXPIRY_OPTIONS=[{label:"5 minutes",seconds:300},{label:"1 hour",seconds:3600},{label:"24 hours",seconds:86400},{label:"7 days",seconds:604800}];}),__zero_define("./src/lib/format.ts",function(exports,__zero_require){function e(e){if(!Number.isFinite(e)||e<0)return"—";if(e<1024)return`${e} B`;let t=["KB","MB","GB","TB","PB"],r=e/1024,n=0;for(;r>=1024&&n<t.length-1;)r/=1024,n+=1;return`${r.toFixed(1)} ${t[n]}`;}exports.humanBytes=e,exports.groupDigits=function(e){return String(e).replace(/\B(?=(\d{3})+(?!\d))/g,",");},exports.statusClass=function(e){return e>=500?"err":e>=400?"warn":e>=300?"redirect":"ok";},exports.bytesCell=function(t){return t.bytes_in>0?`↑ ${e(t.bytes_in)}`:t.bytes_out>0?`↓ ${e(t.bytes_out)}`:"—";},exports.targetOf=function(e){return e.bucket&&e.key?`${e.bucket}/${e.key}`:e.bucket?e.bucket:"—";},exports.middleTruncate=function(e,t=48){if(e.length<=t)return e;let r=Math.floor((t-1)/2);return`${e.slice(0,r)}…${e.slice(e.length-r)}`;},exports.fmtDate=function(e){if(!e)return"—";let t=new Date(e);if(Number.isNaN(t.getTime()))return"—";let r=e=>String(e).padStart(2,"0");return`${t.getFullYear()}-${r(t.getMonth()+1)}-${r(t.getDate())} ${r(t.getHours())}:${r(t.getMinutes())}`;},exports.baseName=function(e){let t=e.endsWith("/")?e.slice(0,-1):e,r=t.lastIndexOf("/");return r>=0?t.slice(r+1):t;};}),__zero_define("./.zero/components/index.ts",function(exports,__zero_require){exports.Avatar=__zero_require("./.zero/components/Avatar.ts").default,exports.Badge=__zero_require("./.zero/components/Badge.ts").default,exports.Button=__zero_require("./.zero/components/Button.ts").default,exports.Card=__zero_require("./.zero/components/Card.ts").default,exports.Checkbox=__zero_require("./.zero/components/Checkbox.ts").default,exports.Combobox=__zero_require("./.zero/components/Combobox.ts").default,exports.createForm=__zero_require("./.zero/components/form.ts").createForm;let e=__zero_require("./.zero/components/rules.ts");exports.email=e.email,exports.intRange=e.intRange,exports.maxLength=e.maxLength,exports.minLength=e.minLength,exports.pattern=e.pattern,exports.required=e.required,exports.Dialog=__zero_require("./.zero/components/Dialog.ts").default,exports.Drawer=__zero_require("./.zero/components/Drawer.ts").default,exports.Input=__zero_require("./.zero/components/Input.ts").default,exports.Pagination=__zero_require("./.zero/components/Pagination.ts").default,exports.Radio=__zero_require("./.zero/components/Radio.ts").default,exports.Select=__zero_require("./.zero/components/Select.ts").default,exports.Spinner=__zero_require("./.zero/components/Spinner.ts").default,exports.Tabs=__zero_require("./.zero/components/Tabs.ts").default,exports.Table=__zero_require("./.zero/components/Table.ts").default,exports.TextArea=__zero_require("./.zero/components/TextArea.ts").default,exports.Toast=__zero_require("./.zero/components/Toast.ts").default,exports.Toggle=__zero_require("./.zero/components/Toggle.ts").default;}),__zero_define("./.zero/components/Toggle.ts",function(exports,__zero_require){let{html:e}=__zero_require("zero"),{ariaDescribedBy:t,ariaInvalid:r,debounce:n,errorNode:o,nativeRef:a,uniqueId:s}=__zero_require("./.zero/components/_internal.ts");exports.default=function(l){let i=l.checked,u=n(()=>i.set(!i.val),l.debounceMs??0),c=a(l.attrs,l.autofocus),d=s("toggle-error");return e`<label class="toggle"><input ref=${c} type="checkbox" class="toggle-input" role="switch" checked=${()=>i.val} aria-checked=${()=>String(i.val)} disabled=${l.disabled??!1} aria-invalid=${r(l.error)} aria-describedby=${t(l.error,d)} @change=${u} /><span class="toggle-track"><span class="toggle-thumb"></span></span><span class="toggle-label">${l.label??""}</span></label>${o(l.error,d)}`;};}),__zero_define("./.zero/components/_internal.ts",function(exports,__zero_require){let{html:e}=__zero_require("zero");function t(e){return"object"==typeof e&&null!==e&&"val"in e;}let r=0;exports.nativeRef=function(e,t){if(null==e&&!0!==t)return{el:null};let r=null;return{get el(){return r;},set el(v){if(r=v,null==v)return;Promise.resolve().then(()=>{if(r===v){for(let[t,r]of Object.entries(e??{}))!1===r||v.hasAttribute(t)||v.setAttribute(t,!0===r?"":String(r));!0===t&&v.focus();}});}};},exports.isReactive=t,exports.read=function(e){return t(e)?e.val:e;},exports.uniqueId=function(e){return r+=1,`${e}-${r}`;},exports.errorNode=function(t,r){return e`${()=>t&&null!=t.val?e`<small class="text-muted" id=${r} data-field-error="">${t.val}</small>`:e``}`;},exports.ariaInvalid=function(e){return()=>e?.val!=null?"true":"false";},exports.ariaDescribedBy=function(e,t){return()=>e?.val!=null?t:"";},exports.debounce=function(e,t){if(!(t>0))return e;let r=null;return(...n)=>{null!=r&&clearTimeout(r),r=setTimeout(()=>e(...n),t);};};}),__zero_define("./.zero/components/Toast.ts",function(exports,__zero_require){let{html:e,effect:t}=__zero_require("zero");exports.default=function(r){let n=r.variant??"info",o=`toast toast-${n}`;return null!=r.duration&&t(()=>{if(!r.open.val)return;let e=setTimeout(()=>{r.open.set(!1),r.onDismiss?.();},r.duration);return()=>clearTimeout(e);}),e`${()=>r.open.val?e`<div class=${o} role="status" aria-live="polite">${r.message}</div>`:null}`;};}),__zero_define("./.zero/components/TextArea.ts",function(exports,__zero_require){let{html:e}=__zero_require("zero"),{ariaDescribedBy:t,ariaInvalid:r,debounce:n,errorNode:o,nativeRef:a,uniqueId:s}=__zero_require("./.zero/components/_internal.ts");exports.default=function(l){let i=n(e=>{let t=e.target;l.value.set(t.value);},l.debounceMs??0),u=a(l.attrs,l.autofocus),c=l.label?e`<label class="textarea-label">${l.label}</label>`:null,d=s("textarea-error");return e`${c}<textarea ref=${u} class="textarea" rows=${l.rows??4} placeholder=${l.placeholder??""} disabled=${l.disabled??!1} aria-invalid=${r(l.error)} aria-describedby=${t(l.error,d)} @input=${i}>${()=>l.value.val}</textarea>${o(l.error,d)}`;};}),__zero_define("./.zero/components/Table.ts",function(exports,__zero_require){let{html:e,each:t,computed:r}=__zero_require("zero"),n=__zero_require("./.zero/components/Spinner.ts").default;exports.default=function(o){let a=o.density??"cozy",s="function"==typeof o.onRowClick,l=o.columns.some(e=>null!=e.width),i=["table",`table-${a}`].concat(s?["table-clickable"]:[]).join(" "),u=o.loading,c=u?()=>i+(u.val?" table-loading":""):i,d=o.maxHeight?`max-height: ${o.maxHeight}; overflow-y: auto`:null;if(o.columns.some(e=>!0===e.sortable)&&null==o.sort)throw Error("Table: at least one column has sortable: true but no sort prop was passed. Pass sort: Signal<SortState | null> from the parent.");let p=o.sort,f=e=>{var t;if(!p)return;let r=(t=p.val,null===t||t.key!==e?{key:e,dir:"asc"}:"asc"===t.dir?{key:e,dir:"desc"}:null);p.set(r),o.onSortChange?.(r);},_=o.columns.map(t=>{let r,n;return r="table-th"+(t.align?` table-align-${t.align}`:""),n=t.width?`width: ${t.width}`:null,!0!==t.sortable?e`<th class=${r} style=${n}>${t.label}</th>`:e`<th class=${r} style=${n} aria-sort=${()=>{let e=p?.val;return e&&e.key===t.key?"asc"===e.dir?"ascending":"descending":"none";}}><button type="button" class="button button-ghost button-sm table-sort-btn" @click=${()=>f(t.key)}>${t.label}<span class="table-sort-icon" aria-hidden="true">${()=>{let e=p?.val;return e&&e.key===t.key?"asc"===e.dir?"▲":"▼":"↕";}}</span></button></th>`;}),h=null==o.onSortChange&&null!=p?r(()=>(function(e,t,r){var n;if(null===t)return e;let o=r.find(e=>e.key===t.key);if(!o)return e;let a=o.compare??(n=o.key,(e,t)=>{let r=e[n],o=t[n],a=null==r,s=null==o;return a&&s?0:a?1:s?-1:"number"==typeof r&&"number"==typeof o?r-o:"string"==typeof r&&"string"==typeof o?r.localeCompare(o):String(r).localeCompare(String(o));}),s="desc"===t.dir?-1:1;return[...e].sort((e,t)=>s*a(e,t));})(o.rows.val,p.val,o.columns)):o.rows;return e`<div class=${c} style=${d}><table class=${l?"table-fixed":""}><thead><tr>${_}</tr></thead><tbody>${t(h,(t,r)=>{let n=s?()=>o.onRowClick(t,r):null,a=o.columns.map(n=>{let o="table-td"+(n.align?` table-align-${n.align}`:""),a=n.render?n.render(t,r):t[n.key];return e`<td class=${o}>${a}</td>`;});return e`<tr class="table-row" data-row-index=${r} @click=${n}>${a}</tr>`;},o.rowKey)}${()=>{if(0!==h.val.length)return null;let t=o.empty??e`<span class="text-muted">No data</span>`;return e`<tr class="table-empty"><td colspan=${o.columns.length}>${t}</td></tr>`;}}</tbody></table>${()=>u&&u.val?e`<div class="table-loading-overlay">${n({size:"md"})}</div>`:null}</div>`;};}),__zero_define("./.zero/components/Spinner.ts",function(exports,__zero_require){let{html:e}=__zero_require("zero");exports.default=function(t={}){let r=t.variant??"primary",n=t.size??"md",o=`spinner spinner-${r} spinner-${n}`,a=t.label?e`<span class="visually-hidden">${t.label}</span>`:null;return e`<span class=${o} role="status">${a}</span>`;};}),__zero_define("./.zero/components/Tabs.ts",function(exports,__zero_require){let{html:e}=__zero_require("zero");exports.default=function(t){let r=e=>{let r=(e%t.tabs.length+t.tabs.length)%t.tabs.length;t.active.set(t.tabs[r].id);},n=t.tabs.map(r=>e`<button class="tabs-tab" role="tab" aria-selected=${()=>t.active.val===r.id} @click=${()=>t.active.set(r.id)}>${r.label}</button>`);return e`<div class="tabs"><div class="tabs-list" role="tablist" @keydown=${e=>{let n,o=(n=t.active.val,t.tabs.findIndex(e=>e.id===n));switch(e.key){case"ArrowLeft":r(o-1);break;case"ArrowRight":r(o+1);break;case"Home":r(0);break;case"End":r(t.tabs.length-1);}}}>${n}</div><div class="tabs-panel" role="tabpanel">${()=>t.panels[t.active.val]??null}</div></div>`;};}),__zero_define("./.zero/components/Select.ts",function(exports,__zero_require){let{html:e}=__zero_require("zero"),{ariaDescribedBy:t,ariaInvalid:r,debounce:n,errorNode:o,nativeRef:a,uniqueId:s}=__zero_require("./.zero/components/_internal.ts");exports.default=function(l){let i=l.size??"md",u=`select select-${i}`,c=n(e=>{let t=e.target;l.value.set(t.value),l.onChange?.(t.value);},l.debounceMs??0),d=l.label?e`<label class="select-label">${l.label}</label>`:null,p=l.options.map(t=>e`<option value=${t.value} selected=${()=>l.value.val===t.value}>${t.label}</option>`),f=a(l.attrs,l.autofocus),_=s("select-error");return e`${d}<select ref=${f} class=${u} disabled=${l.disabled??!1} aria-invalid=${r(l.error)} aria-describedby=${t(l.error,_)} @change=${c}>${p}</select>${o(l.error,_)}`;};}),__zero_define("./.zero/components/Radio.ts",function(exports,__zero_require){let{html:e}=__zero_require("zero"),{ariaDescribedBy:t,ariaInvalid:r,debounce:n,errorNode:o,nativeRef:a,uniqueId:s}=__zero_require("./.zero/components/_internal.ts");exports.default=function(l){let i=n(()=>l.selected.set(l.value),l.debounceMs??0),u=a(l.attrs,l.autofocus),c=s("radio-error");return e`<label class="radio"><input ref=${u} type="radio" name=${l.name} value=${l.value} checked=${()=>l.selected.val===l.value} disabled=${l.disabled??!1} aria-invalid=${r(l.error)} aria-describedby=${t(l.error,c)} @change=${i} /><span class="radio-label">${l.label??""}</span></label>${o(l.error,c)}`;};}),__zero_define("./.zero/components/Pagination.ts",function(exports,__zero_require){let{html:e}=__zero_require("zero"),{read:t}=__zero_require("./.zero/components/_internal.ts");function r(e,t){return t<e?[]:Array.from({length:t-e+1},(t,r)=>e+r);}exports.default=function(n){let o=n.size??"md",a=n.siblingCount??1,s=n.boundaryCount??1,l=n.prevLabel??"Previous",i=n.nextLabel??"Next",u=`button button-${o} pagination-btn`,c=`${u} button-ghost`,d=`${u} button-primary pagination-active`,p=()=>Math.max(1,t(n.totalPages)),f=()=>{let e=p(),t=n.page.val;return t<1?1:t>e?e:t;},_=()=>!0===t(n.disabled)||1>=p(),h=e=>{if(_())return;let t=p(),r=e<1?1:e>t?t:e;r!==f()&&(n.page.set(r),n.onChange?.(r));},m=n.summary?()=>e`<div class="pagination-summary text-small">${n.summary(f(),p())}</div>`:null;return e`
    <nav class=${()=>`pagination pagination-${o} stack gap-sm${_()?" pagination-disabled":""}`} role="navigation" aria-label="Pagination">
      ${m}
      <ul class="pagination-list cluster gap-xs">${()=>{let t,n,o,u,m,b=f(),g=p(),$=_(),y=(t=r(1,Math.min(s,g)),n=r(Math.max(g-s+1,s+1),g),o=Math.max(Math.min(b-a,g-s-2*a-1),s+2),u=Math.min(Math.max(b+a,s+2*a+2),n.length>0?n[0]-2:g-1),m=[...t],o>s+2?m.push("..."):s+1<g-s&&m.push(s+1),m.push(...r(o,u)),u<g-s-1?m.push("..."):g-s>s&&m.push(g-s),m.push(...n),m).map(t=>{let r;return"..."===t?e`
    <li><span class="pagination-ellipsis text-muted" aria-hidden="true">…</span></li>
  `:(r=t===b,e`
    <li>
      <button
        class=${r?d:c}
        aria-label=${`Page ${t}`}
        aria-current=${r?"page":null}
        disabled=${$}
        @click=${()=>h(t)}
      >${t}</button>
    </li>
  `);});return[e`
    <li>
      <button
        class=${`${c} pagination-prev`}
        aria-label=${l}
        disabled=${$||b<=1}
        @click=${()=>h(b-1)}
      >‹</button>
    </li>
  `,...y,e`
    <li>
      <button
        class=${`${c} pagination-next`}
        aria-label=${i}
        disabled=${$||b>=g}
        @click=${()=>h(b+1)}
      >›</button>
    </li>
  `];}}</ul>
    </nav>
  `;};}),__zero_define("./.zero/components/Input.ts",function(exports,__zero_require){let{html:e}=__zero_require("zero"),{ariaDescribedBy:t,ariaInvalid:r,debounce:n,errorNode:o,nativeRef:a,uniqueId:s}=__zero_require("./.zero/components/_internal.ts");exports.default=function(l){let i=l.type??"text",u=l.size??"md",c=`input input-${u}`,d=n(e=>{let t=e.target;l.value.set(t.value),l.onChange?.(t.value);},l.debounceMs??0),p=a(l.attrs,l.autofocus),f=l.label?e`<label class="input-label">${l.label}</label>`:null,_=s("input-error");return e`${f}<input ref=${p} class=${c} type=${i} value=${()=>l.value.val} placeholder=${l.placeholder??""} disabled=${l.disabled??!1} aria-invalid=${r(l.error)} aria-describedby=${t(l.error,_)} @input=${d}>${o(l.error,_)}`;};}),__zero_define("./.zero/components/Drawer.ts",function(exports,__zero_require){let{html:e}=__zero_require("zero");exports.default=function(t){let r=t.mode??"overlay",n=t.size??"md",o=t.side,{open:a}=t,s="push"===r?"drawer-push":"drawer-overlay",l=()=>`drawer ${s} drawer-${o} drawer-${n}`+(a.val?" drawer-open":""),i=e=>{let t="function"==typeof e?e():e;return null==t||""===t;},u=e`
    <header class="drawer-title" hidden=${()=>i(t.title)}>${t.title}</header>
    <div class="drawer-body" hidden=${()=>i(t.body)}>${t.body}</div>
    <footer class="drawer-controls" hidden=${()=>i(t.controls)}>${t.controls}</footer>`,c="overlay"===r?e`<div class=${()=>"drawer-backdrop"+(a.val?" drawer-backdrop-open":"")}></div>`:null,d="overlay"===r?e`<aside class=${l} role="dialog" aria-modal="true">${u}</aside>`:e`<aside class=${l} role="complementary">${u}</aside>`;return e`${c}${d}`;};}),__zero_define("./.zero/components/Dialog.ts",function(exports,__zero_require){let{html:e,effect:t}=__zero_require("zero");exports.default=function(r){let n=r.size??"md",o=`dialog dialog-${n} stack pad-lg`,a=()=>{r.open.set(!1),r.onClose?.();};t(()=>{if(!r.open.val)return;let e=e=>{"Escape"===e.key&&a();};return document.addEventListener("keydown",e),()=>document.removeEventListener("keydown",e);});let s=r.title?e`<h2 class="text-h2">${r.title}</h2>`:null;return e`${()=>r.open.val?e`<div class="dialog-backdrop dialog-open" @click=${a}><div class=${o} role="dialog" aria-modal="true" @click.stop=${()=>{}}>${s}<div class="dialog-body">${r.children??""}</div></div></div>`:null}`;};}),__zero_define("./.zero/components/rules.ts",function(exports,__zero_require){function e(e){return"string"==typeof e?{message:e,allowEmpty:!0}:{message:e?.message,allowEmpty:e?.allowEmpty??!0};}function t(e){return""===e.trim();}exports.required=function(e){return r=>t(r)?e??"This field is required.":null;},exports.intRange=function(r,n,o){let{message:a,allowEmpty:s}=e(o),l=`Must be a whole number between ${r} and ${n}.`;return e=>{if(s&&t(e))return null;let o=e.trim();if(!/^[+-]?\d+$/.test(o))return a??l;let i=Number(o);return r<=i&&i<=n?null:a??l;};},exports.email=function(r){let{message:n,allowEmpty:o}=e(r);return e=>o&&t(e)||/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(e.trim())?null:n??"Enter a valid email address.";},exports.pattern=function(r,n){let{message:o,allowEmpty:a}=e(n),s=new RegExp(r.source,r.flags.replace(/[gy]/g,""));return e=>a&&t(e)||s.test(e)?null:o??"Invalid format.";},exports.maxLength=function(r,n){let{message:o,allowEmpty:a}=e(n),s=`Must be ${r} character${1===r?"":"s"} or fewer.`;return e=>a&&t(e)||e.trim().length<=r?null:o??s;},exports.minLength=function(r,n){let{message:o,allowEmpty:a}=e(n),s=`Must be at least ${r} character${1===r?"":"s"}.`;return e=>a&&t(e)||e.trim().length>=r?null:o??s;};}),__zero_define("./.zero/components/form.ts",function(exports,__zero_require){let{computed:e,signal:t}=__zero_require("zero"),{HttpError:r}=__zero_require("zero/http");exports.createForm=function(n){let o=Object.keys(n.fields),a={},s={},l=t(null),i={};for(let e of o){var u;i[e]=null==(u=n.fields[e].validate)?[]:Array.isArray(u)?u:[u];}let c=()=>{let e={};for(let t of o)e[t]=a[t].val;return e;},d=()=>{let e=c(),t={};for(let r of o)for(let n of i[r]){let o=n(e[r],e);if(null!=o){t[r]=o;break;}}if(n.validate){let r=n.validate(e);for(let e of o){let n=r[e];null!=n&&null==t[e]&&(t[e]=n);}}return t;};for(let e of o)s[e]=function(e,r,n,o){let a=t(e.fields[r].initial);n[r]=a;let s=t(null),l=t(!1),i=()=>{l.set(!0),null!=s.val&&s.set(o()[r]??null);};return{value:{get val(){return a.val;},set(e){a.set(e),i();},update(e){a.update(e),i();}},error:s,touched:l};}(n,e,a,d);let p=e(()=>{let e=d();return o.every(t=>null==e[t]);}),f=e=>{for(let t of o)s[t].error.set(e[t]??null);};return{fields:s,isValid:p,error:l,values:c,reset:()=>{for(let e of o)a[e].set(n.fields[e].initial),s[e].error.set(null),s[e].touched.set(!1);l.set(null);},setErrors:f,submit:e=>async t=>{for(let e of(t.preventDefault(),o))s[e].touched.set(!0);let n=d();if(f(n),l.set(null),!o.some(e=>null!=n[e]))try{await e(c());}catch(e){!function(e,t,n,o){if(e instanceof r&&(400===e.status||409===e.status)){let r=e.body,a=null!=r&&"object"==typeof r?r.errors:void 0;if(null!=a&&"object"==typeof a&&!Array.isArray(a)&&Object.keys(a).length>0){let e=new Set(t),r=[];for(let[t,o]of Object.entries(a))e.has(t)?n[t].error.set(String(o)):r.push(String(o));r.length>0&&o.set(r.join(" "));return;}}o.set("Could not save. Try again.");}(e,o,s,l);}}};};}),__zero_define("./.zero/components/Combobox.ts",function(exports,__zero_require){let{html:e,signal:t,effect:r,ref:n}=__zero_require("zero"),{ariaDescribedBy:o,ariaInvalid:a,errorNode:s,nativeRef:l,read:i,uniqueId:u}=__zero_require("./.zero/components/_internal.ts"),c=0;function d(e,t,r){let n=e.inputRef.el;if(null==n)return;let o=t.toLowerCase(),a=r.find(e=>e.label.toLowerCase().startsWith(o));a&&t.length>0?(n.value=a.label,n.setSelectionRange?.(t.length,a.label.length)):n.value=t;}function p(e,t){e.props.value.set(t.value),e.lastLabel.set(t.label),e.highlight.set(-1),e.open.set(!1);let r=e.inputRef.el;null!=r&&(r.value=t.label,r.setSelectionRange?.(t.label.length,t.label.length)),e.props.onChange?.(t.value,t);}function f(e){let t=e.inputRef.el;if(null==t)return;let r=t.value.trim();if(r===e.lastLabel.val){t.value=r,e.open.set(!1),e.highlight.set(-1);return;}let n=r.toLowerCase(),o=e.options.val.find(e=>e.label.toLowerCase()===n);o?p(e,o):(e.props.value.set(r),e.lastLabel.set(r),t.value=r,e.open.set(!1),e.highlight.set(-1),e.props.onChange?.(r,{value:r,label:r}));}function _(e){e.allowCustom?f(e):function(e){let t=e.inputRef.el;if(null!=t){let r=t.value;e.options.val.some(e=>e.label===r)||(t.value=e.lastLabel.val);}e.open.set(!1),e.highlight.set(-1);}(e);}function h(e,t){let r=e.options.val;if(0===r.length)return;!e.open.val&&e.resolved.val&&e.open.set(!0);let n=(e.highlight.val+t+r.length)%r.length;e.highlight.set(n);let o=r[n];o&&d(e,e.query.val,[o]);}function m(){}exports.default=function(b){let g=b.size??"md",$=++c,y=`combobox-input-${$}`,x=`combobox-list-${$}`,z=e=>`combobox-option-${$}-${e}`,w={props:b,debounceMs:b.debounceMs??200,allowCustom:b.allowCustom??!1,minQueryLength:b.minQueryLength??1,noResultsLabel:b.noResultsLabel??"No results",loadingLabel:b.loadingLabel??"Loading…",query:t(""),options:t([]),highlight:t(-1),open:t(!1),busy:t(!1),lastLabel:t(b.initialLabel??""),resolved:t(!1),inputRef:l(b.attrs,b.autofocus),state:{timer:null,serial:0,lastPrefix:"",allowGhost:!1}},k=n();r(()=>{if(!w.open.val)return;let e=e=>{let t=k.el;if(!t)return;let r=e.target;r&&t.contains?.(r)||_(w);};return document.addEventListener("mousedown",e),()=>document.removeEventListener("mousedown",e);}),r(()=>{!0===i(w.props.disabled)&&(w.open.set(!1),w.highlight.set(-1));});let q=b.label?e`<label class="combobox-label" for=${y}>${b.label}</label>`:null,S=u("combobox-error");return e`
    <div
      class=${()=>{let e;return e=`combobox combobox-${g}`,w.open.val&&(e+=" combobox-open"),!0===i(w.props.disabled)&&(e+=" combobox-disabled"),e;}}
      ref=${k}
      role="combobox"
      aria-haspopup="listbox"
      aria-expanded=${()=>w.open.val?"true":"false"}
      aria-owns=${x}
    >
      ${q}
      <div class="combobox-field">
        <input
          ref=${w.inputRef}
          class=${`input input-${g} combobox-input`}
          id=${y}
          type="text"
          role="combobox"
          autocomplete="off"
          aria-autocomplete="both"
          aria-controls=${x}
          aria-activedescendant=${()=>w.highlight.val>=0?z(w.highlight.val):null}
          placeholder=${b.placeholder??""}
          aria-invalid=${a(b.error)}
          aria-describedby=${o(b.error,S)}
          value=${()=>w.lastLabel.val}
          disabled=${()=>!0===i(b.disabled)}
          @input=${e=>(function(e,t){if(!0===i(e.props.disabled))return;let r=t.target,n=r.selectionStart,o=r.value.slice(0,n??r.value.length);e.state.allowGhost=o.length>e.state.lastPrefix.length,e.state.lastPrefix=o,e.query.set(o),function(e,t){if(!0!==i(e.props.disabled)){if(null!=e.state.timer&&clearTimeout(e.state.timer),++e.state.serial,t.length<e.minQueryLength){e.options.set([]),e.busy.set(!1),e.highlight.set(-1),e.open.set(!1);return;}e.state.timer=setTimeout(()=>{var r,n;let o;return r=e,n=t,o=r.state.serial,void(r.busy.set(!0),r.open.set(!0),r.props.loadOptions(n).then(e=>{var t,a,s,l;return t=r,a=n,s=o,l=e,void(s===t.state.serial&&(t.busy.set(!1),t.resolved.set(!0),t.options.set(l),t.highlight.set(l.length>0?0:-1),t.state.allowGhost&&d(t,a,l)));},()=>{var e;o===(e=r).state.serial&&(e.busy.set(!1),e.resolved.set(!0),e.options.set([]),e.highlight.set(-1));}));},e.debounceMs);}}(e,o);})(w,e)}
          @keydown=${e=>(function(e,t){if(!0!==i(e.props.disabled)){let r,n;if("ArrowDown"===t.key)return void(t.preventDefault(),h(e,1));if("ArrowUp"===t.key)return void(t.preventDefault(),h(e,-1));if("Enter"===t.key)return void function(e,t){t.preventDefault();let r=e.options.val[e.highlight.val];if(!e.allowCustom){r&&p(e,r);return;}let n=e.inputRef.el;r&&n&&n.value===r.label?p(e,r):f(e);}(e,t);if("Escape"===t.key)return void(t.preventDefault(),e.open.set(!1),e.highlight.set(-1));"Tab"===t.key&&(r=e.options.val[e.highlight.val],n=e.inputRef.el,r&&n&&n.value===r.label?(t.preventDefault(),p(e,r)):e.open.set(!1));}})(w,e)}
          @focus=${()=>{!0!==i(w.props.disabled)&&w.resolved.val&&w.options.val.length>0&&w.open.set(!0);}}
          @blur=${()=>_(w)}
        >
        <span class="combobox-spinner" hidden=${()=>!w.busy.val} aria-hidden="true"></span>
      </div>
      <ul
        class="combobox-list border pad-0"
        id=${x}
        role="listbox"
        hidden=${()=>!w.open.val}
        aria-busy=${()=>w.busy.val?"true":"false"}
      >${()=>(function(t,r){return t.busy.val&&0===t.options.val.length?e`<li class="combobox-loading" aria-busy="true">${t.loadingLabel}</li>`:t.resolved.val&&0===t.options.val.length?e`<li class="combobox-empty" aria-disabled="true">${t.noResultsLabel}</li>`:e`${t.options.val.map((n,o)=>e`
      <li
        class=${()=>"combobox-option"+(t.highlight.val===o?" combobox-option-active":"")}
        id=${r(o)}
        role="option"
        aria-selected=${()=>t.highlight.val===o?"true":"false"}
        @mousedown.prevent=${m}
        @click=${()=>p(t,n)}
      >${n.label}</li>
    `)}`;})(w,z)}</ul>
    </div>${s(b.error,S)}
  `;};}),__zero_define("./.zero/components/Checkbox.ts",function(exports,__zero_require){let{html:e}=__zero_require("zero"),{ariaDescribedBy:t,ariaInvalid:r,debounce:n,errorNode:o,nativeRef:a,uniqueId:s}=__zero_require("./.zero/components/_internal.ts");exports.default=function(l){let i=l.checked,u=n(()=>i.set(!i.val),l.debounceMs??0),c=a(l.attrs,l.autofocus),d=s("checkbox-error");return e`<label class="checkbox"><input ref=${c} type="checkbox" checked=${()=>i.val} disabled=${l.disabled??!1} aria-invalid=${r(l.error)} aria-describedby=${t(l.error,d)} @change=${u} /><span class="checkbox-label">${l.label??""}</span></label>${o(l.error,d)}`;};}),__zero_define("./.zero/components/Card.ts",function(exports,__zero_require){let{html:e}=__zero_require("zero");exports.default=function(t={}){let r=t.variant??"surface",n=`card card-${r}`,o=t.title?e`<h3 class="card-title">${t.title}</h3>`:null;return e`<section class=${n}>${o}<div class="card-body">${t.children??""}</div></section>`;};}),__zero_define("./.zero/components/Button.ts",function(exports,__zero_require){let{html:e}=__zero_require("zero");exports.default=function(t={}){let r=t.variant??"primary",n=t.size??"md",o=t.type??"button",a=`button button-${r} button-${n}`,s=`button-spinner spinner spinner-${r} spinner-sm`,l=t.loading?e`<span class=${s} role="status" aria-label="loading"></span>`:null,i=(t.disabled??!1)||(t.loading??!1);return e`<button class=${a} type=${o} form=${t.form} name=${t.name} value=${t.value} disabled=${i} @click=${e=>{i||t.onClick?.(e);}}>${l}${t.children??""}</button>`;};}),__zero_define("./.zero/components/Badge.ts",function(exports,__zero_require){let{html:e}=__zero_require("zero");exports.default=function(t={}){let r=t.variant??"default",n=t.size??"md",o=`badge badge-${r} badge-${n}`;return e`<span class=${o}>${t.children??""}</span>`;};}),__zero_define("./.zero/components/Avatar.ts",function(exports,__zero_require){let{html:e}=__zero_require("zero");exports.default=function(t){let r=t.size??"md";if(t.src){let n=`avatar avatar-${r}`;return e`<img class=${n} src=${t.src} alt=${t.alt}>`;}let n=`avatar avatar-${r} avatar-initials`,o=t.initials??t.alt[0]?.toUpperCase()??"";return e`<span class=${n} aria-label=${t.alt}>${o}</span>`;};}),__zero_define("./src/routes/live-log.ts",function(exports,__zero_require){let{computed:e,each:t,effect:r,html:n,ref:o,signal:a}=__zero_require("zero"),{Input:s,Select:l}=__zero_require("./.zero/components/index.ts"),{bytesCell:i,statusClass:u,targetOf:c}=__zero_require("./src/lib/format.ts"),{appendCapped:d,elapsedLabel:p,matchesFilter:f}=__zero_require("./src/lib/log.ts");exports.default=function(){var _,h,m,b,g;let $,y,x,z,w,k,q,S,T,A,E,C,N,R,L,j,O=(q=a([]),S=a(!1),T=a(0),A=a(""),E=a("all"),C=a("any"),N=a(null),R=o(),L=(_=q,h=S,m=T,b=R,$=0,y=!0,x=[],z=!1,w=e=>{0!==e.length&&(0===$&&e[0]&&($=e[0].ts),_.update(t=>d(t,e,2e3)),y&&!h.val&&requestAnimationFrame(()=>{b.el&&(b.el.scrollTop=b.el.scrollHeight);}));},k=()=>{if(z=!1,0===x.length)return;if(h.val)return void m.set(x.length);let e=x;x=[],w(e);},r(()=>{let e=new EventSource("/_/api/events");return e.onmessage=e=>{try{let t=JSON.parse(e.data);"number"==typeof t.id&&(x.push(t),z||(z=!0,requestAnimationFrame(k)));}catch{}},()=>e.close();}),{origin:()=>$,resume:()=>{h.set(!1),m.set(0);let e=x;x=[],y=!0,w(e);},onScroll:()=>{let e=b.el;e&&(y=e.scrollTop+e.clientHeight>=e.scrollHeight-8);}}),j=e(()=>q.val.filter(e=>f(e,A.val,E.val,C.val))),{events:q,visible:j,paused:S,newCount:T,filter:A,statusFilter:E,authFilter:C,expanded:N,scroller:R,origin:L.origin,resume:L.resume,onScroll:L.onScroll});return n`
    <section class="screen log-screen flex-col">
      ${function(e){let{filter:t,statusFilter:r,authFilter:o,visible:a,events:i,paused:u,newCount:c,resume:d}=e;return n`
    <div class="toolbar split align-center pad-md border-b">
      <div class="cluster align-center gap-md">
        <div class="toolbar-filter">
          ${s({value:t,placeholder:"Filter by op, key, method",size:"sm"})}
        </div>
        ${l({value:r,options:[{value:"all",label:"All status"},{value:"2",label:"2xx"},{value:"3",label:"3xx"},{value:"4",label:"4xx"},{value:"5",label:"5xx"}],size:"sm"})}
        ${l({value:o,options:[{value:"any",label:"Any auth"},{value:"header",label:"Header"},{value:"presigned",label:"Presigned"},{value:"anonymous",label:"Anonymous"}],size:"sm"})}
      </div>
      <div class="cluster align-center gap-md">
        <span class="count mono">${()=>`${a.val.length} / ${i.val.length}`}</span>
        <button class=${()=>"pause-btn"+(u.val?" paused":"")} @click=${()=>{u.val?d():u.set(!0);}}>
          ${()=>u.val?`▶ ${c.val} new`:"❚❚ Pause"}
        </button>
      </div>
    </div>
  `;}(O)}
      <div class="log-wrap" ref=${O.scroller} @scroll=${O.onScroll}>
        ${g=O,n`
    <table class="log-table">
      <thead>
        <tr>
          <th class="c-time text-start">TIME</th>
          <th class="c-method text-start">METHOD</th>
          <th class="c-op text-start">OPERATION</th>
          <th class="c-key text-start">BUCKET / KEY</th>
          <th class="c-status text-start">STATUS</th>
          <th class="c-dur text-start">DUR</th>
          <th class="c-bytes text-start">BYTES</th>
        </tr>
      </thead>
      <tbody>
        ${t(g.visible,e=>{var t,r,o;let a,s;return t=e,r=g.expanded,o=g.origin(),a=p(t.ts,o),s="c-dur"+(t.duration_ms>=100?" slow":""),n`
    <tr class="log-row" @click=${()=>r.update(e=>e===t.id?null:t.id)}>
      <td class="c-time mono">${a}</td>
      <td class="c-method"><span class=${"method m-"+t.method.toLowerCase()}>${t.method}</span></td>
      <td class="c-op">${t.op??"—"}</td>
      <td class="c-key mono" title=${c(t)}>${c(t)}</td>
      <td class="c-status">
        <span class=${"pill s-"+u(t.status)}>${t.status}</span>
      </td>
      <td class=${s+" mono"}>${t.duration_ms} ms</td>
      <td class="c-bytes mono">${i(t)}</td>
    </tr>
    <tr class=${()=>"log-detail"+(r.val===t.id?" open":"")}>
      ${()=>{var e;let o;return r.val===t.id?(e=t,o=(e,t)=>n`<div class="kv"><span class="k">${e}</span><span class="v mono">${t}</span></div>`,n`
    <td colspan="7">
      <div class="detail-grid grid">
        ${o("op",e.op??"—")}
        ${o("auth",e.auth)}
        ${o("error_code",e.error_code??"—")}
        ${o("bytes_in",String(e.bytes_in))}
        ${o("bytes_out",String(e.bytes_out))}
        ${o("duration",e.duration_ms+" ms")}
        ${o("id",String(e.id))}
      </div>
    </td>
  `):"";}}
    </tr>
  `;},e=>e.id)}
      </tbody>
    </table>
  `}
        ${()=>0===O.visible.val.length?n`<div class="empty-state text-center">Waiting for S3 traffic…</div>`:""}
      </div>
    </section>
  `;};}),__zero_define("./src/lib/log.ts",function(exports,__zero_require){let{targetOf:e}=__zero_require("./src/lib/format.ts");exports.matchesFilter=function(t,r,n,o){if("all"!==n&&Math.floor(t.status/100)!==Number(n)||"any"!==o&&t.auth!==o)return!1;let a=r.trim().toLowerCase();return!a||`${t.method} ${t.op??""} ${e(t)}`.toLowerCase().includes(a);},exports.appendCapped=function(e,t,r){if(0===t.length)return e;let n=e.concat(t);return n.length>r?n.slice(n.length-r):n;},exports.elapsedLabel=function(e,t){let r=Math.max(0,(e-t)/1e3);return`${r.toFixed(2)}s`;};}),__zero_define("./src/components/chrome.ts",function(exports,__zero_require){let{html:e,route:t}=__zero_require("zero"),{health:r,healthy:n,theme:o,toggleTheme:a}=__zero_require("./src/stores/chrome.ts");exports.default=function(s){let l,i;return e`
    <div class="app-shell">
      ${e`
    <header class="topbar split align-center pad-md border-b">
      <div class="cluster align-center gap-lg">
        <div class="cluster align-center gap-sm">
          <span class="brand-mark" aria-hidden="true">◆</span>
          <span class="brand-name text-h4">cubby</span>
          <span class="badge-version mono">${()=>"v"+(r.val?.version??"…")}</span>
        </div>
        <div class="cluster align-center gap-xs">
          <span class="chrome-label">DATA-DIR</span>
          <span class="chrome-value mono">${()=>r.val?.data_dir??"…"}</span>
        </div>
        <div class="cluster align-center gap-xs">
          <span class="chrome-label">ENDPOINT</span>
          <span class="chrome-value mono">${()=>r.val?.endpoint??"…"}</span>
        </div>
      </div>
      <div class="cluster align-center gap-md">
        <span class="cluster align-center gap-xs">
          <span class=${()=>"status-dot "+(n.val?"ok":"down")}></span>
          <span class="status-text">${()=>n.val?"healthy":"offline"}</span>
        </span>
        <button
          class="theme-toggle"
          @click=${a}
          aria-label="Toggle light/dark theme"
        >
          ${()=>"dark"===o.val?"☀":"☾"}
        </button>
      </div>
    </header>
  `}
      <div class="app-body flank gap-0">
        ${l=t(),i="nav-item split align-center",e`
    <nav class="nav stack justify-between border-r pad-md">
      <div class="stack gap-xs">
        <div class="nav-heading">INSPECT</div>
        <a class=${()=>i+("/_"===l.path||"/_/"===l.path?" active":"")} href="/_/">
          <span>Live request log</span>
          <span class="live-dot" aria-hidden="true"></span>
        </a>
        <a class=${()=>i+(l.path.startsWith("/_/browser")?" active":"")} href="/_/browser">Bucket browser</a>
      </div>
      <div class="nav-footer stack gap-xs">
        <div class="mono">
          <b>${()=>r.val?.bucket_count??0}</b> buckets ·
          <b>${()=>r.val?.object_count??0}</b> objects
        </div>
        <div class="mono muted">region ${()=>r.val?.region??"us-east-1"}</div>
      </div>
    </nav>
  `}
        <main class="app-main stack gap-0">${s.outlet}</main>
      </div>
    </div>
  `;};}),__zero_require("./src/app.ts");