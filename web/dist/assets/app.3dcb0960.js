let __zero_modules={},__zero_cache={};function __zero_define(e,t){__zero_modules[e]=t;}function __zero_require(e){if(__zero_cache[e])return __zero_cache[e].exports;let t={exports:{}};return __zero_cache[e]=t,__zero_modules[e](t.exports,__zero_require),t.exports;}__zero_define("zero",function(exports,__zero_require){let e=[],t=null,r=new Set;function n(t){let r=e[e.length-1];r&&(t.add(r),r._sources.add(t));}function o(e){for(let t of e._sources)t.delete(e);e._sources.clear();}function a(e){let t=e,r=new Set;return{get val(){return n(r),t;},set(e){if(e!==t)for(let n of(t=e,[...r]))n._notify();},update(e){this.set(e(t));}};}function s(n){let a,s=t,l={_sources:new Set,_notify(){i();}};function i(){o(l),a&&(a(),a=void 0),e.push(l);try{let e=n();"function"==typeof e&&(a=e);}finally{e.pop();}}function u(){o(l),a&&(a(),a=void 0),s&&s._effects.delete(u),r.delete(u);}return t?t._effects.add(u):r.add(u),i(),u;}let l=function(){let e=t,r={_effects:new Set,_children:new Set,_cleanups:[],dispose(){for(let e of[...r._effects])e();for(let e of(r._effects.clear(),[...r._children]))e.dispose();for(let e of(r._children.clear(),r._cleanups))try{e();}catch(e){}r._cleanups.length=0,e&&e._children.delete(r);},onCleanup(e){r._cleanups.push(e);},run(n){let o=t;t=r,e&&e._children.add(r);try{return n();}finally{t=o;}}};return r;},i=new WeakMap,u="TEXT",c="TAG_OPEN",p="TAG_NAME",d="IN_TAG",f="ATTR_NAME",h="AFTER_ATTR_NAME",_="ATTR_VALUE_UNQUOTED",m="ATTR_VALUE_DQ",b="ATTR_VALUE_SQ",g="CLOSING_TAG",$="http://www.w3.org/2000/svg";function x(e,...t){let r=i.get(e);return r||(r=function(e){let t=document.createDocumentFragment(),r=[],n=u,o=t,a=[],s=[],l="",i="",x=!1,y=null,w=!1,z="",k="",q=0;function S(e){return q>0||"svg"===e.toLowerCase()?document.createElementNS($,e):document.createElement(e);}function T(){k&&(o.appendChild(document.createTextNode(k)),k="");}function A(){l&&(w?(y.push(i),r.push({type:"attr",path:[...s],name:l,statics:y})):o.setAttribute(l,i),l="",i="",y=null,w=!1);}for(let E=0;E<e.length;E++){let C=e[E];for(let e=0;e<C.length;e++){let r=C[e],y=C[e+1];switch(n){case u:"<"===r?(T(),"/"===y?(n=g,e++):(n=c,z="")):k+=r;break;case c:if(/[a-zA-Z]/.test(r))n=p,z=r;else throw Error(`html: unexpected char '${r}' after '<'`);break;case p:if(/[a-zA-Z0-9\-]/.test(r))z+=r;else if(" "===r||"	"===r||"\n"===r||"\r"===r){let e=S(z);e.namespaceURI===$&&q++,o.appendChild(e),a.push({el:e,pathIdx:o.childNodes.length-1,svg:e.namespaceURI===$}),s.push(o.childNodes.length-1),o=e,n=d;}else if(">"===r){let e=S(z);e.namespaceURI===$&&q++,o.appendChild(e),a.push({el:e,pathIdx:o.childNodes.length-1,svg:e.namespaceURI===$}),s.push(o.childNodes.length-1),o=e,n=u;}else if("/"===r&&">"===y){let t=S(z);o.appendChild(t),e++,n=u;}else throw Error(`html: unexpected char '${r}' in tag name`);break;case d:">"===r?n=u:"/"===r&&">"===y?(a.pop(),s.pop(),o=a.length>0?a[a.length-1].el:t,e++,n=u):" "!==r&&"	"!==r&&"\n"!==r&&"\r"!==r&&(n=f,l=r,x=!1);break;case f:"="===r?(n=h,x=!0):" "===r||"	"===r||"\n"===r||"\r"===r?n=h:">"===r?(A(),n=u):l+=r;break;case h:'"'===r?n=m:"'"===r?n=b:">"===r?(A(),n=u):"="===r?x=!0:" "===r||"	"===r||"\n"===r||"\r"===r||(x?(n=_,i=r):(A(),n=d,e--));break;case m:'"'===r?(A(),n=d):i+=r;break;case b:"'"===r?(A(),n=d):i+=r;break;case _:" "===r||"	"===r||"\n"===r||"\r"===r?(A(),n=d):">"===r?(A(),n=u):i+=r;break;case g:if(">"===r){let e=a.pop();e&&e.svg&&q--,s.pop(),o=a.length>0?a[a.length-1].el:t,n=u;}}}if(E<e.length-1)switch(T(),n){case u:{let e=document.createComment("");o.appendChild(e),r.push({type:"node",path:[...s,o.childNodes.length-1]});break;}case h:case m:case b:case _:{let e=[...s];if(l.startsWith("@")){let[t,...o]=l.slice(1).split(".");r.push({type:"event",path:e,event:t,modifiers:o}),l="",i="",n===h&&(n=d);}else"ref"===l?(r.push({type:"ref",path:e}),l="",i="",n===h&&(n=d)):(null===y&&(y=[]),y.push(i),i="",w=!0,n===h&&(n=_));break;}default:throw Error(`html: placeholder in unsupported position (state: ${n})`);}}return T(),{fragment:t,parts:r};}(e),i.set(e,r)),{_template:r,_values:t};}let y={enter:"Enter",escape:"Escape",space:" ",tab:"Tab",up:"ArrowUp",down:"ArrowDown",left:"ArrowLeft",right:"ArrowRight"};function w(e){if(null==e||"object"!=typeof e)return!1;let t=Object.getOwnPropertyDescriptor(e,"val");return!!t&&"function"==typeof t.get;}function z(e,t,r){let n=e.tagName;if("value"===t&&("INPUT"===n||"TEXTAREA"===n||"SELECT"===n)){let t=null==r?"":String(r);return e.value!==t&&(e.value=t),!0;}return"checked"===t&&"INPUT"===n?(e.checked=!!r&&"false"!==r,!0):"selected"===t&&"OPTION"===n&&(e.selected=!!r&&"false"!==r,!0);}function k(e,t,r){z(e,t,r)||(!1===r||null==r?e.removeAttribute(t):!0===r?e.setAttribute(t,""):e.setAttribute(t,String(r)));}function q(e,t,r,n){let o=r[0];for(let e=0;e<n.length;e++)o+=function e(t){return null==t?"":w(t)?e(t.val):"function"==typeof t?e(t()):String(t);}(n[e])+r[e+1];z(e,t,o)||e.setAttribute(t,o);}function S(e,t){return 0===t.currentNodes.length?e.nextSibling:t.currentNodes[t.currentNodes.length-1].nextSibling;}function T(e){for(let t of e.currentNodes)t.parentNode&&t.parentNode.removeChild(t);e.currentNodes.length=0;}function A(e){if(e.itemScopes){for(let t of e.itemScopes)t.dispose();e.itemScopes.length=0;}}function E(e,t,r){if(null==t)return;if(null!=t&&"object"==typeof t&&null!=t._template&&Array.isArray(t._values)){let n=document.createDocumentFragment();for(L(t,n);n.childNodes.length>0;){let t=n.childNodes[0];e.parentNode.insertBefore(t,S(e,r)),r.currentNodes.push(t);}return;}let n=document.createTextNode(String(t));e.parentNode.insertBefore(n,S(e,r)),r.currentNodes.push(n);}function C(e,t,r){if(T(r),null!=t){if(Array.isArray(t)){for(let n of t)E(e,n,r);return;}E(e,t,r);}}function j(e,t){for(let r of e){if(r===t)return 100;if(r.startsWith(t+":")){let e=r.slice(t.length+1);if(!/^\d+$/.test(e))throw Error(`html: invalid modifier '${r}' — expected '${t}:<ms>' with positive integer`);let n=Number(e);if(n<=0)throw Error(`html: invalid modifier '${r}' — interval must be > 0`);return n;}}return 0;}function L(e,t){let{_template:r,_values:n}=e,o=r.fragment.cloneNode(!0),a=r.parts.map(e=>(function(e,t){let r=e;for(let e of t)r=r.childNodes[e];return r;})(o,e.path)),i=0;for(let e=0;e<r.parts.length;e++){let t=r.parts[e],o=a[e];switch(t.type){case"attr":{var u,c,p;let e=t.statics.length-1;u=t.name,c=t.statics,p=n.slice(i,i+e),2===c.length&&""===c[0]&&""===c[1]?function(e,t,r){w(r)?s(()=>k(e,t,r.val)):"function"==typeof r?s(()=>k(e,t,r())):k(e,t,r);}(o,u,p[0]):function(e,t,r,n){n.some(e=>w(e)||"function"==typeof e)?s(()=>q(e,t,r,n)):q(e,t,r,n);}(o,u,c,p),i+=e;break;}case"event":!function(e,t,r,n){let o,a,l,i,u,c,p=(o=r.filter(e=>e in y),a=r.includes("prevent"),l=r.includes("stop"),i=j(r,"throttle"),u=j(r,"debounce"),c=e=>{if(!(o.length>0)||o.some(t=>e.key===y[t]))return a&&e.preventDefault?.(),l&&e.stopPropagation?.(),n(e);},i>0&&(c=function(e,t){let r=0;return(...n)=>{let o=Date.now();if(!(o-r<t))return r=o,e(...n);};}(c,i)),u>0&&(c=function(e,t){let r;return(...n)=>{clearTimeout(r),r=setTimeout(()=>e(...n),t);};}(c,u)),c),d=r.includes("once")?{once:!0}:void 0;e.addEventListener(t,p,d),s(()=>()=>e.removeEventListener(t,p,d));}(o,t.event,t.modifiers,n[i]),i++;break;case"ref":!function(e,t){t.el=e,s(()=>()=>{t.el=null;});}(o,n[i]),i++;break;case"node":!function(e,t,r){if(A(r),T(r),null!=t){if(t&&t._isEach)return function(e,t,r){if("function"==typeof t.keyFn)return function(e,t,r){let{signal:n,renderFn:o,keyFn:a}=t;r.itemsByKey=r.itemsByKey||Object.create(null),s(()=>{let t=n.val;if(!Array.isArray(t)){for(let e in r.itemsByKey)r.itemsByKey[e].scope.dispose();r.itemsByKey=Object.create(null),T(r);return;}let s=Array(t.length),i=Object.create(null);for(let e=0;e<t.length;e++){let r=String(a(t[e],e));if(i[r])throw Error(`each: duplicate key '${r}' in row ${e}`);i[r]=!0,s[e]=r;}let u=r.itemsByKey,c=Object.create(null),p=e.parentNode;for(let e in u)if(!i[e]){let t=u[e];for(let e of(t.scope.dispose(),t.nodes))e.parentNode&&e.parentNode.removeChild(e);}let d=[],f=e.nextSibling;for(let e=0;e<t.length;e++){let r=s[e],n=u[r];if(null==n){let r=l(),a=[];r.run(()=>{let r=o(t[e],e),n=document.createDocumentFragment();for(L(r,n);n.childNodes.length>0;){let e=n.childNodes[0];p.insertBefore(e,f),a.push(e),d.push(e);}}),n={scope:r,nodes:a};}else for(let e of n.nodes)e!==f?p.insertBefore(e,f):f=e.nextSibling,d.push(e);c[r]=n,n.nodes.length>0&&(f=n.nodes[n.nodes.length-1].nextSibling);}r.itemsByKey=c,r.currentNodes=d;});}(e,t,r);let{signal:n,renderFn:o}=t;r.itemScopes=r.itemScopes||[],s(()=>{A(r),T(r);let t=n.val;if(Array.isArray(t))for(let n=0;n<t.length;n++){let a=l();r.itemScopes.push(a),a.run(()=>{let a=o(t[n],n),s=document.createDocumentFragment();for(L(a,s);s.childNodes.length>0;){let t=s.childNodes[0];e.parentNode.insertBefore(t,S(e,r)),r.currentNodes.push(t);}});}});}(e,t,r);if(w(t))return s(()=>C(e,t.val,r));if("function"==typeof t)return s(()=>C(e,t(),r));C(e,t,r);}}(o,n[i],{currentNodes:[]}),i++;}}t.appendChild(o);}function N(e){return"/"===e?e:e.endsWith("/")?e.slice(0,-1):e;}function R(e){if(!e||"?"===e)return{};let t=e.startsWith("?")?e.slice(1):e,r={};for(let e of t.split("&")){let t=e.indexOf("=");-1===t?r[decodeURIComponent(e)]="":r[decodeURIComponent(e.slice(0,t))]=decodeURIComponent(e.slice(t+1));}return r;}function M(e){let t=e.indexOf("#"),r=t>=0?e.slice(0,t):e,n=r.indexOf("?");return n>=0?{pathname:r.slice(0,n),search:r.slice(n)}:{pathname:r,search:""};}function I(e,t){let{pathname:r,search:n}=M(t),o=N(r),a=R(n);for(let t of e){let e=function(e,t){let r=e.regex.exec(t);if(!r)return null;let n={};for(let t=0;t<e.paramNames.length;t++)n[e.paramNames[t]]=decodeURIComponent(r[t+1]);return{params:n};}(t.compiled,o);if(e)return{route:t,params:e.params,query:a,pathname:o,search:n};}return null;}let B=null;function O({pattern:e,normalized:t,loaderOrLoad:r,opts:n}){return{pattern:e,normalized:t,loaderOrLoad:r,opts:n,resolvedComponent:null};}exports.signal=a,exports.computed=function(t){let r={_value:void 0,_dirty:!0,_subscribers:new Set},a={_sources:new Set,_notify(){if(!r._dirty)for(let e of(r._dirty=!0,[...r._subscribers]))e._notify();},get val(){return r._dirty&&function(t,r,n){o(t),e.push(t);try{r._value=n();}finally{e.pop();}r._dirty=!1;}(a,r,t),n(r._subscribers),r._value;}};return a;},exports.effect=s,exports.html=x,exports.commit=L,exports.each=function(e,t,r){return{_isEach:!0,signal:e,renderFn:t,keyFn:r};},exports.ref=function(){return{el:null};},exports.App=class{constructor(){this._state=new Map,this._routes=[],this._layout=null,this._pathSig=a(""),this._paramsSig=a({}),this._querySig=a({}),this._mountEl=null,this._running=!1,this._rootSlotSig=a(null),this._rootScope=null,this._stateProxy=new Proxy({},{get:(e,t)=>this._state.get(t)}),this._middleware=[],this._navToken=0,this._loading=null,this._error=null,this._navScope=null,this._chain=[],this._lastCommittedUrl=null;}_computeDivergence(e,t){let r=0;for(;r<e.length&&r<t.length&&e[r].descriptor===t[r];)r++;return r;}_resolveLoadingFor(e,t){for(let r=t;r<e.length;r++)if(e[r].opts.loading)return e[r].opts.loading;return this._loading;}_mergeMeta(e){return e.reduce((e,t)=>Object.assign({},e,t.opts.meta||{}),{});}_slotAt(e){return 0===e?this._rootSlotSig:this._chain[e-1].outletSig;}_assertNotRunning(e){if(this._running)throw Error(`App.${e}() cannot be called after run()`);}state(e,t){if(this._assertNotRunning("state"),this._state.has(e))throw Error(`App.state: key "${e}" already registered`);return this._state.set(e,t),this;}layout(e){if(this._assertNotRunning("layout"),null!=this._layout)throw Error("App.layout: layout already set");if("function"!=typeof e)throw Error("App.layout: component must be a function");return this._layout=e,this;}use(e){if(this._assertNotRunning("use"),"function"!=typeof e)throw Error("App.use: middleware must be a function");return this._middleware.push(e),this;}loading(e){if(this._assertNotRunning("loading"),null!=this._loading)throw Error("App.loading: loading already set");if("function"!=typeof e)throw Error("App.loading: component must be a function");return this._loading=e,this;}error(e){if(this._assertNotRunning("error"),null!=this._error)throw Error("App.error: error already set");if("function"!=typeof e)throw Error("App.error: component must be a function");return this._error=e,this;}route(e,t,r={}){if(this._assertNotRunning("route"),"function"!=typeof t)throw Error("App.route: handler must be a function");if(null!=r.children&&!Array.isArray(r.children))throw Error("App.route: opts.children must be an array");if(null!=r.guard&&"function"!=typeof r.guard)throw Error("App.route: guard must be a function");if(null!=r.load&&"function"!=typeof r.load)throw Error("App.route: load must be a function");if(null!=r.meta&&("object"!=typeof r.meta||Array.isArray(r.meta)))throw Error("App.route: meta must be an object");if(null!=r.loading&&"function"!=typeof r.loading)throw Error("App.route: loading must be a function");if(null!=r.error&&"function"!=typeof r.error)throw Error("App.route: error must be a function");let n=N(e),{children:o,...a}=r,s=O({pattern:e,normalized:n,loaderOrLoad:t,opts:a});return this._flattenRoutes(s,[s],o),this;}_flattenRoutes(e,t,r){if(!r||0===r.length){let{normalized:r}=e;this._routes.push({pattern:e.pattern,normalized:r,compiled:function(e){if("*"===e)return{pattern:e,normalized:"*",paramNames:[],regex:/^.*$/,isWildcard:!0};let t=N(e),r=[],n=RegExp("^"+t.split("/").map(e=>e.startsWith(":")?(r.push(e.slice(1)),"([^/]+)"):e.replace(/[.*+?^${}()|[\]\\]/g,"\\$&")).join("\\/")+"$");return{pattern:e,normalized:t,paramNames:r,regex:n,isWildcard:!1};}(r),loader:e.loaderOrLoad,opts:e.opts,resolvedComponent:null,chain:t});return;}for(let n of r){if("function"!=typeof n.load)throw Error("App.route: each child entry must have a load function");let{children:r,...o}=n,a=function(e,t){let r=N(e);return"/"===t?r:"/"===r?N(t):N(r+t);}(e.normalized,n.path),s=O({pattern:n.path,normalized:a,loaderOrLoad:n.load,opts:o});this._flattenRoutes(s,[...t,s],r);}}match(e){return I(this._routes,e);}run(e){if(this._running)throw Error("App.run: already running");let t=document.querySelector(e);if(!t)throw Error(`App.run: element not found for selector "${e}"`);this._mountEl=t,this._running=!0,B=this,this._rootScope=l(),this._rootScope.run(()=>{this._layout?L(this._layout({outlet:this._rootSlotSig}),this._mountEl):L(x`${this._rootSlotSig}`,this._mountEl);});let r=window.location.pathname+window.location.search;this._navigateTo(r);let n=()=>this._navigateTo(window.location.pathname+window.location.search);this._popstateListener=n,window.addEventListener("popstate",n),this._rootScope.onCleanup(()=>window.removeEventListener("popstate",n));let o=e=>(function(e){var t;let r;if(e.defaultPrevented||null!=e.button&&0!==e.button||e.metaKey||e.ctrlKey||e.shiftKey||e.altKey)return;let n=e.target;for(;n&&"A"!==n.tagName;)n=n.parentNode;if(!n)return;let o=n.getAttribute("target");if(o&&"_self"!==o||n.hasAttribute("download")||n.hasAttribute("data-external"))return;let a=n.getAttribute("href");!(!a||a.startsWith("#")||/^[a-z][a-z0-9+\-.]*:/i.test(a)&&!a.startsWith(window.location.origin))&&(e.preventDefault(),t=a.startsWith(window.location.origin)?a.slice(window.location.origin.length):a,(r=B)&&(window.history.pushState(null,"",t),r._navigateTo(t)));})(e);this._clickListener=o,document.addEventListener("click",o),this._rootScope.onCleanup(()=>document.removeEventListener("click",o));}_navigateTo(e){var t;let r=++this._navToken,n=I(this._routes,e);if(n)this._pathSig.set(n.pathname),this._paramsSig.set(n.params),this._querySig.set(n.query);else{let{pathname:t,search:r}=M(e);this._pathSig.set(N(t)),this._paramsSig.set({}),this._querySig.set(R(r));}if(null==n)return void this._rootSlotSig.set(null);this._navScope&&(this._navScope.dispose(),this._navScope=null),this._navScope=l();let o=new AbortController;this._navScope.onCleanup(()=>o.abort());let s=(t=o.signal,(e,r={})=>{let n=r.signal,o=n?function(e,t){if("u">typeof AbortSignal&&"function"==typeof AbortSignal.any)return AbortSignal.any([e,t]);let r=new AbortController,n=e=>()=>{r.abort(e.reason);};return e.aborted?r.abort(e.reason):e.addEventListener("abort",n(e)),t.aborted?r.abort(t.reason):t.addEventListener("abort",n(t)),r.signal;}(t,n):t;return globalThis.fetch(e,{...r,signal:o});}),i=this;(async()=>{let t=i._stateProxy,u=n.route.chain,c=u.length,p=i._computeDivergence(i._chain,u);p=Math.min(p,c-1);let d=i._slotAt(p),f=i._mergeMeta(u),h={path:n.pathname,params:n.params,query:n.query,meta:f},_=i._resolveLoadingFor(u,p),m=setTimeout(()=>{r!==i._navToken||_&&i._navScope.run(()=>{d.set(_());});},150);try{let e;for(let e of i._middleware){let n=!1,o=(e,t={})=>{n=!0,i._navToken++,window.history.replaceState(null,"",e),i._navigateTo(e);};if(await e({route:h,state:t,redirect:o}),r!==i._navToken||n)return void clearTimeout(m);}for(let e=p;e<c;e++){let o=u[e];if(o.opts.guard){let e=(e,t={})=>{i._navToken++,window.history.replaceState(null,"",e),i._navigateTo(e);},a=await o.opts.guard({params:n.params,query:n.query,state:t,route:h,redirect:e});if(r!==i._navToken)return void clearTimeout(m);if(!1===a){clearTimeout(m),null!=i._lastCommittedUrl&&window.history.replaceState(null,"",i._lastCommittedUrl);return;}}if(o.opts.load&&(await o.opts.load({params:n.params,query:n.query,state:t,fetch:s,route:h}),r!==i._navToken))return void clearTimeout(m);}clearTimeout(m);for(let e=i._chain.length-1;e>=p;e--)i._chain[e].scope.dispose();i._chain.length=p;let o=[];for(let s=c-1;s>=p;s--){let p,d=u[s],f=s===c-1?null:a(e),h=l();if(null==d.resolvedComponent){let e=d.loaderOrLoad({params:n.params,query:n.query,state:t,...null!=f?{outlet:f}:{}});if(null!=e&&"function"==typeof e.then){let o=await e;if(r!==i._navToken)return void clearTimeout(m);d.resolvedComponent=o.default,h.run(()=>{p=d.resolvedComponent({params:n.params,query:n.query,state:t,...null!=f?{outlet:f}:{}});});}else d.resolvedComponent=d.loaderOrLoad,h.run(()=>{p=e;});}else h.run(()=>{p=d.resolvedComponent({params:n.params,query:n.query,state:t,...null!=f?{outlet:f}:{}});});o.unshift({descriptor:d,scope:h,outletSig:f}),e=p;}for(let e of o)i._chain.push(e);i._chain[p].scope.run(()=>{d.set(e);}),i._lastCommittedUrl=n.pathname+n.search,function(e,t,r){for(let n of e.querySelectorAll("a")){let e,o,a=n.getAttribute("href");if(!a||a.startsWith("#")){n.removeAttribute("data-active"),n.removeAttribute("data-active-exact");continue;}if(a.startsWith("/")){let t=a.indexOf("?");t>=0?(e=a.slice(0,t),o=a.slice(t)):(e=a,o="");}else if(a.startsWith(window.location.origin)){let t=a.slice(window.location.origin.length),r=t.indexOf("?");r>=0?(e=t.slice(0,r),o=t.slice(r)):(e=t,o="");}else{n.removeAttribute("data-active"),n.removeAttribute("data-active-exact");continue;}let s=(e=N(e))===t&&o===r,l=t===e||t.startsWith(e+"/");s?(n.setAttribute("data-active-exact",""),n.setAttribute("data-active","")):(l?n.setAttribute("data-active",""):n.removeAttribute("data-active"),n.removeAttribute("data-active-exact"));}}(i._mountEl,n.pathname,n.search);}catch(t){if(r!==i._navToken)return;if(t&&"AbortError"===t.name&&o.signal.aborted)return void clearTimeout(m);if(clearTimeout(m),i._error){i._navScope.dispose(),i._navScope=l();let r=()=>i._navigateTo(e);i._navScope.run(()=>{d.set(i._error({error:t,retry:r}));}),i._chain[p]={descriptor:null,scope:i._navScope,outletSig:null},i._chain.length=p+1;}else console.error("navigation error",t);}})();}_getState(e){if(!this._state.has(e))throw Error(`inject: key "${e}" is not registered`);return this._state.get(e);}},exports.inject=function(e){if(null==B)throw Error("inject: no app is running");return B._getState(e);},exports.navigate=function(e,t={}){let r=B;if(!r)throw Error("navigate: no app is running");let n=t.state??null;t.replace?window.history.replaceState(n,"",e):window.history.pushState(n,"",e),r._navigateTo(e);},exports.back=function(){if(!B)throw Error("back: no app is running");window.history.back();},exports.forward=function(){if(!B)throw Error("forward: no app is running");window.history.forward();},exports.route=function(){let e=B;if(!e)throw Error("route: no app is running");return{get path(){return e._pathSig.val;},get params(){return e._paramsSig.val;},get query(){return e._querySig.val;}};},exports._setCurrentApp=function(e){B=e;},exports._createScope=l,exports._getCurrentApp=function(){return B;},exports._disposeUnownedEffects=function(){for(let e of[...r])e();r.clear();};}),__zero_define("zero/http",function(exports,__zero_require){class e extends Error{constructor(e,t,r){super(`HTTP ${e} ${t}`),this.name="HttpError",this.status=e,this.statusText=t,this.body=r;}}async function t(e,t,n){let o=e=>async r=>e>=t.length?n(r):t[e](r,o(e+1));return r(await o(0)(e));}async function r(e){let t=e.headers.get("Content-Type")||"",r=/\bjson\b/i.test(t),a=""===t;if(!e.ok)return n(e,r,a);if(r)return e.json();if(a){let{parsed:t,value:r}=await o(e);return t?r:e;}return e;}async function n(t,r,n){let a;if(r)try{a=await t.json();}catch(e){a=void 0;}else if(n){let{parsed:e,value:r,text:n}=await o(t);a=e?r:n;}else try{a=await t.text();}catch(e){a=void 0;}throw new e(t.status,t.statusText,a);}async function o(e){let t;try{t=await e.text();}catch(e){return{parsed:!1,value:void 0,text:""};}try{return{parsed:!0,value:JSON.parse(t),text:t};}catch(e){return{parsed:!1,value:void 0,text:t};}}exports.createHttp=function(e={}){let r=e.fetch??globalThis.fetch,n=[];function o(e,o,a,s){return function(e,r,n,o,a,s){let{fetch:l,...i}=o??{},u={...i,method:e},c=new Headers(u.headers||{});return void 0!==n&&(function(e){if(null===e||"object"!=typeof e||"u">typeof FormData&&e instanceof FormData||"u">typeof Blob&&e instanceof Blob||e instanceof ArrayBuffer||ArrayBuffer.isView(e)||"u">typeof URLSearchParams&&e instanceof URLSearchParams||"u">typeof ReadableStream&&e instanceof ReadableStream)return!1;let t=Object.getPrototypeOf(e);return t===Object.prototype||null===t;}(n)||Array.isArray(n)?(c.has("Content-Type")||c.set("Content-Type","application/json"),u.body=JSON.stringify(n)):u.body=n),u.headers=c,t(new Request(r,u),a,l??s);}(e,o,a,s,n,r);}let a={use(e){if("function"!=typeof e)throw TypeError("HttpClient.use: middleware must be a function");return n.push(e),a;},get:(e,t)=>o("GET",e,void 0,t),post:(e,t,r)=>o("POST",e,t,r),put:(e,t,r)=>o("PUT",e,t,r),patch:(e,t,r)=>o("PATCH",e,t,r),delete:(e,t)=>o("DELETE",e,void 0,t),request:(e,o)=>(function(e,r,n,o){let{fetch:a,...s}=r??{};return t(e instanceof Request&&0===Object.keys(s).length?e:new Request(e,s),n,a??o);})(e,o,n,r)};return a;},exports.HttpError=e;}),__zero_define("./src/app.ts",function(exports,__zero_require){let{App:e,effect:t,route:r}=__zero_require("zero"),n=__zero_require("./src/components/chrome.ts").default,o=__zero_require("./src/routes/live-log.ts").default,{default:a,load:s}=__zero_require("./src/routes/browser.ts"),{applyTheme:l,loadHealth:i,watchSystemTheme:u}=__zero_require("./src/stores/chrome.ts"),{syncBrowseFromUrl:c}=__zero_require("./src/stores/browse.ts");l(),u(),i(),new e().layout(n).route("/_/",o).route("/_",o).route("/_/browser",a,{load:s}).route("*",o).run("#app"),t(()=>{let e=r(),t=e.path;e.query,"/_/browser"===t&&c();});}),__zero_define("./src/stores/browse.ts",function(exports,__zero_require){let{navigate:e,route:t,signal:r}=__zero_require("zero"),{createBucket:n,deleteObject:o,getMeta:a,listBuckets:s,listObjects:l,presign:i,search:u,uploadObject:c}=__zero_require("./src/lib/api.ts"),{locationToUrl:p,parentPrefix:d,uploadKey:f,urlToLocation:h}=__zero_require("./src/lib/browse.ts"),{loadHealth:_}=__zero_require("./src/stores/chrome.ts"),m=r([]),b=r(null),g=r(""),$=r(null),x=r(""),y=r(!1),w=r(null),z=r(null),k=r(null),q=r(null);async function S(){let e=await s();m.set(e.buckets);}function T(t,r=!1){try{e(p(t),r?{replace:!0}:void 0);}catch{}}async function A(e){await n(e),await S(),await E(e),await _();}async function E(e){b.set(e),g.set(""),x.set(""),w.set(null),z.set(null),k.set(null),q.set(null),T({bucket:e,prefix:"",object:null}),await j();}async function C(e){g.set(e),z.set(null),k.set(null),q.set(null),T({bucket:b.val,prefix:e,object:null}),await j();}async function j(){let e=b.val;e&&$.set(await l(e,g.val));}async function L(e){(x.set(e),0===e.trim().length)?w.set(null):await R();}async function N(){y.set(!y.val),x.val.trim().length>0&&await R();}async function R(){let e=y.val?null:b.val;w.set(await u(x.val,e));}async function M(e,t){b.set(e),g.set(d(t)),z.set(t),k.set(null),q.set(null),T({bucket:e,prefix:d(t),object:t}),k.set(await a(e,t));}async function I(e){let t=b.val;if(t){for(let r of e)await c(t,f(g.val,r.name),r);await j(),await S(),await _();}}async function B(e){let t=b.val;t&&(await o(t,e),await j(),await S(),await _());}async function O(e,t){let r=b.val,n=z.val;if(!r||!n)return;let o=await i({method:e,bucket:r,key:n,expires_in_s:t});q.set(o.url);}async function D(e){if(!e.bucket){0===m.val.length&&await S();let e=m.val[0];e&&T({bucket:e.name,prefix:"",object:null},!0);return;}let t=b.val!==e.bucket,r=g.val!==e.prefix;t&&(x.set(""),w.set(null)),b.set(e.bucket),g.set(e.prefix),(t||r||null===$.val)&&await j(),null===e.object?(z.set(null),k.set(null),q.set(null)):(z.val!==e.object||null===k.val)&&(z.set(e.object),k.set(null),q.set(null),k.set(await a(e.bucket,e.object)));}exports.loadBuckets=S,exports.createBucket=A,exports.selectBucket=E,exports.navigateTo=C,exports.loadFolder=j,exports.setSearch=L,exports.toggleAllBuckets=N,exports.runSearch=R,exports.openObject=M,exports.closeObject=function(){z.set(null),k.set(null),q.set(null),T({bucket:b.val,prefix:g.val,object:null});},exports.uploadFiles=I,exports.removeObject=B,exports.generatePresign=O,exports.applyLocation=D,exports.syncBrowseFromUrl=function(){queueMicrotask(()=>{let e;try{let r=t();if("/_/browser"!==r.path)return;e=r.query;}catch{return;}D(h(e));});},exports.buckets=m,exports.selectedBucket=b,exports.prefix=g,exports.folder=$,exports.searchTerm=x,exports.allBuckets=y,exports.searchResults=w,exports.selectedObject=z,exports.objectMeta=k,exports.presignedUrl=q;}),__zero_define("./src/stores/chrome.ts",function(exports,__zero_require){let{signal:e}=__zero_require("zero"),{getHealth:t}=__zero_require("./src/lib/api.ts"),r=e(null),n=e(!1);async function o(){try{let e=await t();r.set(e),n.set("ok"===e.status);}catch{n.set(!1);}}let a="cubby:theme",s=["dark","light","system"],l=e(p(function(e){try{return localStorage.getItem(e);}catch{return null;}}(a)));function i(e){return s[(s.indexOf(e)+1)%s.length];}function u(e,t){return"dark"===e||"light"===e?e:t?"dark":"light";}function c(){return u(l.val,!("function"==typeof matchMedia&&matchMedia("(prefers-color-scheme: light)").matches));}function p(e){return"dark"===e||"light"===e||"system"===e?e:"system";}function d(e){l.set(e);try{localStorage.setItem(a,e);}catch{}f();}function f(){document.documentElement.setAttribute("data-theme",c());}exports.loadHealth=o,exports.nextThemePref=i,exports.resolveTheme=u,exports.effectiveTheme=c,exports.parseThemePref=p,exports.setThemePref=d,exports.cycleTheme=function(){d(i(l.val));},exports.applyTheme=f,exports.watchSystemTheme=function(){"function"==typeof matchMedia&&matchMedia("(prefers-color-scheme: dark)").addEventListener("change",()=>{"system"===l.val&&f();});},exports.health=r,exports.healthy=n,exports.THEME_KEY=a,exports.themePref=l;}),__zero_define("./src/lib/api.ts",function(exports,__zero_require){let{createHttp:e}=__zero_require("zero/http"),t=e({fetch:(...e)=>globalThis.fetch(...e)});function r(e){return e.split("/").map(encodeURIComponent).join("/");}async function n(e,t,n){let o=await fetch(`/_/api/buckets/${encodeURIComponent(e)}/objects/${r(t)}`,{method:"PUT",body:n});if(!o.ok)throw Error(`upload failed: ${o.status}`);}exports.getHealth=function(){return t.get("/_/api/health");},exports.clearEvents=function(){return t.post("/_/api/events/clear",{});},exports.listBuckets=function(){return t.get("/_/api/buckets");},exports.createBucket=function(e){return t.post("/_/api/buckets",{name:e});},exports.listObjects=function(e,r,n){let o=new URLSearchParams({delimiter:"/",prefix:r});return n&&o.set("continuation-token",n),t.get(`/_/api/buckets/${encodeURIComponent(e)}/objects?${o}`);},exports.search=function(e,r){let n=new URLSearchParams({q:e});return r&&n.set("bucket",r),t.get(`/_/api/search?${n}`);},exports.getMeta=function(e,n){return t.get(`/_/api/buckets/${encodeURIComponent(e)}/objects/${r(n)}`);},exports.contentUrl=function(e,t){return`/_/api/buckets/${encodeURIComponent(e)}/objects/${r(t)}?content`;},exports.uploadObject=n,exports.deleteObject=function(e,n){return t.delete(`/_/api/buckets/${encodeURIComponent(e)}/objects/${r(n)}`);},exports.presign=function(e){return t.post("/_/api/presign",e);};}),__zero_define("./src/lib/browse.ts",function(exports,__zero_require){function e(e){let t=e.lastIndexOf("/");return t>=0?e.slice(0,t+1):"";}exports.viewMode=function(e){return e.trim().length>0?"search":"folder";},exports.crumbs=function(e,t){let r=[{label:e,prefix:""}],n=t.split("/").filter(e=>e.length>0),o="";for(let e of n)o+=`${e}/`,r.push({label:e,prefix:o});return r;},exports.folderLabel=function(e,t){return e.startsWith(t)?e.slice(t.length):e;},exports.uploadKey=function(e,t){return`${e}${t}`;},exports.parentPrefix=e,exports.locationToUrl=function(e){if(!e.bucket)return"/_/browser";let t=[`bucket=${encodeURIComponent(e.bucket)}`];return null!==e.object?t.push(`object=${encodeURIComponent(e.object)}`):e.prefix&&t.push(`prefix=${encodeURIComponent(e.prefix)}`),`/_/browser?${t.join("&")}`;},exports.urlToLocation=function(t){let r=t.bucket??null;if(!r)return{bucket:null,prefix:"",object:null};let n=t.object??null;return{bucket:r,prefix:null!==n?e(n):t.prefix??"",object:n};},exports.highlightParts=function(e,t){if(!t)return[{text:e,match:!1}];let r=t.toLowerCase(),n=e.toLowerCase(),o=[],a=0,s=n.indexOf(r,a);for(;-1!==s;)s>a&&o.push({text:e.slice(a,s),match:!1}),o.push({text:e.slice(s,s+r.length),match:!0}),a=s+r.length,s=n.indexOf(r,a);return a<e.length&&o.push({text:e.slice(a),match:!1}),o.length>0?o:[{text:e,match:!1}];};}),__zero_define("./src/routes/browser.ts",function(exports,__zero_require){let{each:e,effect:t,html:r,signal:n}=__zero_require("zero"),{Input:o}=__zero_require("./.zero/components/index.ts"),{HttpError:a}=__zero_require("zero/http"),{contentUrl:s}=__zero_require("./src/lib/api.ts"),{crumbs:l,folderLabel:i,highlightParts:u,viewMode:c}=__zero_require("./src/lib/browse.ts"),{baseName:p,fmtDate:d,humanBytes:f,truncateEnd:h}=__zero_require("./src/lib/format.ts"),_=__zero_require("./src/components/object-detail.ts").default,{ArchiveIcon:m,BucketIcon:b,DownloadIcon:g,FileIcon:$,FolderIcon:x,PlusIcon:y,TrashIcon:w}=__zero_require("./src/components/icons.ts"),{allBuckets:z,buckets:k,createBucket:q,folder:S,loadBuckets:T,navigateTo:A,openObject:E,prefix:C,removeObject:j,searchResults:L,searchTerm:N,selectBucket:R,selectedBucket:M,selectedObject:I,setSearch:B,toggleAllBuckets:O,uploadFiles:D}=__zero_require("./src/stores/browse.ts");exports.default=function(){return r`<div class="browser-root stack gap-0">${()=>{let T,P,U,H,W,F,V,K;return I.val?_():r`
    <section class="screen browser-screen flank gap-0">
      ${T=n(!1),P=n(""),U=n(null),H=()=>{T.set(!1),P.set(""),U.set(null);},W=async()=>{let e=P.val.trim();if(e)try{await q(e),H();}catch(e){U.set(function(e){if(e instanceof a){let t=e.body;return t?.error?.message??`Request failed (${e.status})`;}return"Could not create bucket.";}(e));}},r`
    <div class="buckets-col stack gap-0">
      <div class="buckets-head split align-center pad-sm border-b">
        <span class="section-label">BUCKETS</span>
        <button
          class=${()=>"new-bucket-add cluster align-center justify-center"+(T.val?" active":"")}
          @click=${()=>T.val?H():T.set(!0)}
          title="New bucket"
          aria-label="New bucket"
        >${y()}</button>
      </div>
      ${()=>T.val?r`
              <form
                class="new-bucket-form cluster align-center gap-sm pad-sm border-b"
                @submit=${e=>{e.preventDefault(),W();}}
                @keydown=${e=>{"Escape"===e.key&&H();}}
              >
                ${o({value:P,placeholder:"bucket-name",size:"sm",autofocus:!0,error:U})}
                <button class="button button-primary button-sm" type="button" @click=${W}>Create</button>
              </form>
            `:""}
      <div class="buckets-list stack gap-xs pad-sm">
        ${e(k,e=>{let t=e.object_count>0?f(e.size):"—";return r`
      <button class=${()=>"bucket-row stack gap-0 text-start"+(M.val===e.name?" active":"")} @click=${()=>R(e.name)}>
        <span class="bucket-head flank align-center gap-sm">
          <span class="bucket-icon" aria-hidden="true">${b()}</span>
          <span class="bucket-name mono">${e.name}</span>
        </span>
        <span class="bucket-sub mono muted">${e.object_count} objects · ${t}</span>
      </button>
    `;},e=>e.name)}
      </div>
    </div>
  `}
      ${F=n(!1),r`
    <div
      class=${()=>"listing-pane stack gap-0"+(F.val?" dragging":"")}
      @drop=${e=>{e.preventDefault(),F.set(!1);let t=e.dataTransfer?.files;t&&t.length>0&&D(Array.from(t));}}
      @dragover=${e=>{e.preventDefault(),F.set(!0);}}
      @dragleave=${()=>F.set(!1)}
    >
      ${V=n(""),t(()=>V.set(N.val)),K=(e,t)=>r`
    <button
      class=${()=>"seg-btn"+(z.val===t?" active":"")}
      @click=${()=>{z.val!==t&&O();}}
    >${e}</button>
  `,r`
    <div class="listing-toolbar split align-center pad-md border-b">
      <div class="search-group cluster align-center gap-sm">
        <div class="search-field">
          ${o({value:V,placeholder:"Search keys…",size:"sm",onChange:e=>B(e),debounceMs:150})}
        </div>
        <div class="segmented cluster" title="Search scope">
          ${K("This bucket",!1)}${K("All buckets",!0)}
        </div>
      </div>
      <span class="mono muted">
        ${()=>{let e=L.val;return e?`${e.results.length} matches`:"";}}
      </span>
    </div>
  `}
      ${()=>"search"===c(N.val)?r`
    <div class="search-results">
      ${()=>{let e=L.val;if(!e)return r`<div class="pad-lg muted">Searching…</div>`;if(0===e.results.length){let e=N.val;return r`<div class="pad-lg muted">No keys match “${e}”.</div>`;}return r`<table class="listing-table search-table"><tbody>${e.results.map(e=>{var t;let n;return n=u((t=e).key,N.val),r`
    <tr class="listing-row search-row" @click=${()=>E(t.bucket,t.key)}>
      <td class="c-name">
        <span class="cluster align-center gap-sm">
          ${z.val?r`<span class="bucket-tag mono">${t.bucket}</span>`:""}
          <span class="mono">${n.map(e=>e.match?r`<mark>${e.text}</mark>`:e.text)}</span>
        </span>
      </td>
      <td class="c-size mono">${f(t.size)}</td>
      <td class="c-mod mono muted">${d(t.last_modified)}</td>
    </tr>
  `;})}</tbody></table>`;}}
    </div>
  `:r`
    <div class="folder-view">
      ${r`
    <div class="breadcrumb cluster align-center gap-xs pad-md">
      ${()=>{let e=M.val;if(!e)return"";let t=l(e,C.val);return r`${t.map((e,t)=>r`${t>0?r`<span class="crumb-sep muted">/</span>`:""}<button
            class="crumb mono"
            @click=${()=>A(e.prefix)}
          >${e.label}</button>`)}`;}}
    </div>
  `}
      ${()=>{var e,t;let n,o=S.val;return o?0===o.common_prefixes.length&&0===o.objects.length?r`
    <div class="empty-state text-center stack gap-sm align-center justify-center">
      <div class="empty-icon" aria-hidden="true">${m()}</div>
      <div>No objects yet.</div>
      <div class="muted">Drop files to upload to <span class="mono">${()=>`${M.val??""}/${C.val}`}</span></div>
    </div>
  `:(e=o.common_prefixes,t=o.objects,n=C.val,r`
    <table class="listing-table">
      <thead>
        <tr><th class="c-name text-start">NAME</th><th class="c-size text-start">SIZE</th><th class="c-mod text-start">MODIFIED</th><th class="c-etag text-start">ETAG</th></tr>
      </thead>
      <tbody>
        ${e.map(e=>{var t,o;return t=i(e,n),o=e,r`
    <tr class="listing-row folder-row" @click=${()=>A(o)}>
      <td class="c-name"><span class="cluster align-center gap-sm"><span class="folder-icon" aria-hidden="true">${x()}</span><span class="mono">${t}</span></span></td>
      <td class="c-size mono muted">—</td>
      <td class="c-mod mono muted">—</td>
      <td class="c-etag mono muted">—</td>
    </tr>
  `;})}
        ${t.map(e=>{var t;let n;return t=e,n=M.val,r`
    <tr class="listing-row object-row">
      <td class="c-name" @click=${()=>E(n,t.key)}>
        <span class="cluster align-center gap-sm"><span class="file-icon" aria-hidden="true">${$()}</span><span class="mono link">${p(t.key)}</span></span>
      </td>
      <td class="c-size mono">${f(t.size)}</td>
      <td class="c-mod mono muted">${d(t.last_modified)}</td>
      <td class="c-etag mono muted">
        <span class="cluster align-center gap-sm">
          <span class="etag-val" title=${t.etag}>${h(t.etag,10)}</span>
          <a class="row-action row-download" href=${s(n,t.key)} download title="Download" aria-label="Download">${g()}</a>
          <button class="row-action row-delete" @click=${()=>j(t.key)} title="Delete" aria-label="Delete">${w()}</button>
        </span>
      </td>
    </tr>
  `;})}
      </tbody>
    </table>
  `):r`<div class="pad-lg muted">Loading…</div>`;}}
    </div>
  `}
      <div class="drop-overlay align-center justify-center"><span>Drop to upload to ${()=>`${M.val??""}/${C.val}`}</span></div>
    </div>
  `}
    </section>
  `;}}</div>`;},exports.load=function(){return T();};}),__zero_define("./src/components/icons.ts",function(exports,__zero_require){let{html:e}=__zero_require("zero");exports.TrashIcon=function(){return e`<svg class="icon" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M7 21a2 2 0 0 1-2-2V6H4V4h5V3h6v1h5v2h-1v13a2 2 0 0 1-2 2H7ZM9 17h2V8H9v9Zm4 0h2V8h-2v9Z"></path></svg>`;},exports.PauseIcon=function(){return e`<svg class="icon" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><rect x="6" y="5" width="4" height="14" rx="1"></rect><rect x="14" y="5" width="4" height="14" rx="1"></rect></svg>`;},exports.PlayIcon=function(){return e`<svg class="icon" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M8 5v14l11-7L8 5Z"></path></svg>`;},exports.PlusIcon=function(){return e`<svg class="icon" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M19 13h-6v6h-2v-6H5v-2h6V5h2v6h6v2z"></path></svg>`;},exports.DownloadIcon=function(){return e`<svg class="icon" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M19 9h-4V3H9v6H5l7 7 7-7zM5 18v2h14v-2H5z"></path></svg>`;},exports.ChevronLeftIcon=function(){return e`<svg class="icon" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M15.41 7.41 14 6l-6 6 6 6 1.41-1.41L10.83 12z"></path></svg>`;},exports.FolderIcon=function(){return e`<svg class="icon" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M10 4H4c-1.1 0-1.99.9-1.99 2L2 18c0 1.1.9 2 2 2h16c1.1 0 2-.9 2-2V8c0-1.1-.9-2-2-2h-8l-2-2z"></path></svg>`;},exports.FileIcon=function(){return e`<svg class="icon" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M6 2c-1.1 0-1.99.9-1.99 2L4 20c0 1.1.89 2 1.99 2H18c1.1 0 2-.9 2-2V8l-6-6H6zm7 7V3.5L18.5 9H13z"></path></svg>`;},exports.BucketIcon=function(){return e`<svg class="icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linejoin="round" stroke-linecap="round" aria-hidden="true"><path d="M4.5 8l1.6 11.2a1.6 1.6 0 0 0 1.58 1.38h8.64a1.6 1.6 0 0 0 1.58-1.38L19.5 8"></path><ellipse cx="12" cy="8" rx="7.5" ry="2"></ellipse><path d="M6 7.2C6.7 3 9 1.5 12 1.5s5.3 1.5 6 5.7"></path></svg>`;},exports.ArchiveIcon=function(){return e`<svg class="icon" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M20 2H4c-1.1 0-2 .9-2 2v3.01c0 .72.43 1.34 1 1.69V20c0 1.1 1.1 2 2 2h14c.9 0 2-.9 2-2V8.7c.57-.35 1-.97 1-1.69V4c0-1.1-.9-2-2-2zm-5 12H9v-2h6v2zm5-7H4V4h16v3z"></path></svg>`;},exports.MoonIcon=function(){return e`<svg class="icon" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M12 3c-4.97 0-9 4.03-9 9s4.03 9 9 9 9-4.03 9-9c0-.46-.04-.92-.1-1.36-.98 1.37-2.58 2.26-4.4 2.26-2.98 0-5.4-2.42-5.4-5.4 0-1.81.89-3.42 2.26-4.4C12.92 3.04 12.46 3 12 3z"></path></svg>`;},exports.SunIcon=function(){return e`<svg class="icon" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M12 7a5 5 0 1 0 0 10 5 5 0 0 0 0-10zM2 13h2a1 1 0 0 0 0-2H2a1 1 0 0 0 0 2zm18 0h2a1 1 0 0 0 0-2h-2a1 1 0 0 0 0 2zM11 2v2a1 1 0 0 0 2 0V2a1 1 0 0 0-2 0zm0 18v2a1 1 0 0 0 2 0v-2a1 1 0 0 0-2 0zM5.99 4.58a1 1 0 0 0-1.41 1.41l1.06 1.06a1 1 0 0 0 1.41-1.41L5.99 4.58zm12.37 12.37a1 1 0 0 0-1.41 1.41l1.06 1.06a1 1 0 0 0 1.41-1.41l-1.06-1.06zm1.06-10.96a1 1 0 0 0-1.41-1.41l-1.06 1.06a1 1 0 0 0 1.41 1.41l1.06-1.06zM7.05 18.36a1 1 0 0 0-1.41-1.41l-1.06 1.06a1 1 0 0 0 1.41 1.41l1.06-1.06z"></path></svg>`;},exports.SystemIcon=function(){return e`<svg class="icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true"><circle cx="12" cy="12" r="9"></circle><path fill="currentColor" stroke="none" d="M12 3a9 9 0 0 0 0 18Z"></path></svg>`;};}),__zero_define("./src/components/object-detail.ts",function(exports,__zero_require){let{effect:e,html:t,signal:r}=__zero_require("zero"),{Button:n,Select:o}=__zero_require("./.zero/components/index.ts"),{ChevronLeftIcon:a}=__zero_require("./src/components/icons.ts"),{contentUrl:s}=__zero_require("./src/lib/api.ts"),{fmtDate:l,groupDigits:i,humanBytes:u}=__zero_require("./src/lib/format.ts"),{EXPIRY_OPTIONS:c,formatPreview:p,previewKind:d}=__zero_require("./src/lib/preview.ts"),{closeObject:f,generatePresign:h,objectMeta:_,prefix:m,presignedUrl:b,selectedBucket:g,selectedObject:$}=__zero_require("./src/stores/browse.ts");function x(e){e.target.select();}exports.default=function(){var y;let w,z,k,q,S=r(null);return y=S,e(()=>{let e=_.val,t=g.val,r=$.val;if(y.set(null),!e||!t||!r)return;let n=d(e.content_type,e.size);("text"===n||"json"===n||"xml"===n)&&fetch(s(t,r)).then(e=>e.text()).then(e=>y.set(e)).catch(()=>y.set("(failed to load preview)"));}),t`
    <section class="screen detail-screen stack gap-0">
      <header class="detail-topbar split align-center pad-md border-b">
        <button class="crumb-back cluster align-center gap-xs" @click=${f}>
          ${a()}
          <span class="mono">${()=>`${g.val??""}/${m.val}`}</span>
        </button>
        <div class="cluster align-center gap-sm preview-label">
          <span class="chrome-label">PREVIEW</span>
          <span class="mono">${()=>_.val?.content_type??"—"}</span>
        </div>
      </header>
      <div class="detail-body">
        <div class="preview-pane stack gap-0">${()=>(function(e,r){let n=g.val,o=$.val;if(!e||!n||!o)return t`<div class="preview-empty muted">Loading…</div>`;let a=d(e.content_type,e.size);if("image"===a)return t`<img class="preview-img" src=${s(n,o)} alt=${o} />`;if("text"===a||"json"===a||"xml"===a){let e=null===r?"Loading…":p(a,r);return t`<pre class="preview-text mono">${e}</pre>`;}return t`
    <div class="preview-download stack gap-md align-center justify-center">
      <div class="muted">No inline preview for <span class="mono">${e.content_type??"this type"}</span>.</div>
      <a class="button button-secondary button-md" href=${s(n,o)} download>Download</a>
    </div>
  `;})(_.val,S.val)}</div>
        <aside class="meta-pane stack gap-lg pad-lg">
          ${()=>{var e;let r,n;return _.val?(r=Object.entries((e=_.val).metadata??{}),n=(e,r)=>t`<div class="meta-row"><span class="meta-k">${e}</span><span class="meta-v mono">${r}</span></div>`,t`
    <div class="stack gap-md">
      <div>
        <div class="section-label">OBJECT</div>
        <div class="meta-table">
          ${n("size",`${u(e.size)} (${i(e.size)} bytes)`)}
          ${n("content-type",e.content_type??"—")}
          ${n("etag",e.etag)}
          ${n("last-modified",`${l(e.last_modified)} UTC`)}
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
          ${w=r("GET"),z=r(String(c[1].seconds)),k=c.map(e=>({value:String(e.seconds),label:e.label})),q=e=>t`
    <button
      class=${()=>"seg-btn"+(w.val===e?" active":"")}
      @click=${()=>w.set(e)}
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
          <div class="segmented cluster">${q("GET")}${q("PUT")}</div>
        </div>
        <div class="stack gap-xs presign-expiry">
          <span class="chrome-label">EXPIRES IN</span>
          ${o({value:z,options:k,size:"sm"})}
        </div>
      </div>
      ${n({variant:"primary",children:"Generate URL",onClick:()=>h(w.val,Number(z.val))})}
      <input
        class="presign-url mono"
        readonly
        value=${b}
        hidden=${()=>!b.val}
        @focus=${x}
      />
    </div>
  `}
        </aside>
      </div>
    </section>
  `;};}),__zero_define("./src/lib/preview.ts",function(exports,__zero_require){let e=new Set(["application/javascript","application/x-javascript","application/x-sh"]);function t(e){try{return JSON.stringify(JSON.parse(e),null,2);}catch{return e;}}function r(e){let t=e.trim();if(!t.startsWith("<"))return e;let r=t.match(/<[^>]+>|[^<]+/g);if(!r)return e;let o=[],a=[],s=0,l=()=>"  ".repeat(s);for(let t of r){let r=t.trim();if(r)if(r.startsWith("<?")||r.startsWith("<!"))o.push(l()+r);else if(r.startsWith("</")){if(a.pop()!==n(r))return e;s=Math.max(0,s-1),o.push(l()+r);}else r.endsWith("/>")?o.push(l()+r):r.startsWith("<")?(o.push(l()+r),a.push(n(r)),s+=1):o.push(l()+r);}return a.length>0?e:o.join("\n");}function n(e){return e.replace(/^<\/?/,"").replace(/\/?>$/,"").trim().split(/\s/)[0]??"";}exports.previewKind=function(t,r){let n=(t??"").toLowerCase().split(";")[0]?.trim()??"";if(n.startsWith("image/"))return"image";let o="application/json"===n||n.endsWith("+json"),a="application/xml"===n||"text/xml"===n||n.endsWith("+xml");return!(o||a||n.startsWith("text/")||e.has(n))||r>2097152?"none":o?"json":a?"xml":"text";},exports.prettyJson=t,exports.prettyXml=r,exports.formatPreview=function(e,n){return"json"===e?t(n):"xml"===e?r(n):n;},exports.PREVIEW_MAX_BYTES=2097152,exports.EXPIRY_OPTIONS=[{label:"5 minutes",seconds:300},{label:"1 hour",seconds:3600},{label:"24 hours",seconds:86400},{label:"7 days",seconds:604800}];}),__zero_define("./src/lib/format.ts",function(exports,__zero_require){function e(e){if(!Number.isFinite(e)||e<0)return"—";if(e<1024)return`${e} B`;let t=["KB","MB","GB","TB","PB"],r=e/1024,n=0;for(;r>=1024&&n<t.length-1;)r/=1024,n+=1;return`${r.toFixed(1)} ${t[n]}`;}exports.humanBytes=e,exports.groupDigits=function(e){return String(e).replace(/\B(?=(\d{3})+(?!\d))/g,",");},exports.statusClass=function(e){return e>=500?"err":e>=400?"warn":e>=300?"redirect":"ok";},exports.bytesCell=function(t){return t.bytes_in>0?`↑ ${e(t.bytes_in)}`:t.bytes_out>0?`↓ ${e(t.bytes_out)}`:"—";},exports.targetOf=function(e){return e.bucket&&e.key?`${e.bucket}/${e.key}`:e.bucket?e.bucket:"—";},exports.middleTruncate=function(e,t=48){if(e.length<=t)return e;let r=Math.floor((t-1)/2);return`${e.slice(0,r)}…${e.slice(e.length-r)}`;},exports.truncateEnd=function(e,t=10){return e.length>t?`${e.slice(0,t)}…`:e;},exports.fmtDate=function(e){if(!e)return"—";let t=new Date(e);if(Number.isNaN(t.getTime()))return"—";let r=e=>String(e).padStart(2,"0");return`${t.getFullYear()}-${r(t.getMonth()+1)}-${r(t.getDate())} ${r(t.getHours())}:${r(t.getMinutes())}`;},exports.baseName=function(e){let t=e.endsWith("/")?e.slice(0,-1):e,r=t.lastIndexOf("/");return r>=0?t.slice(r+1):t;};}),__zero_define("./.zero/components/index.ts",function(exports,__zero_require){exports.Avatar=__zero_require("./.zero/components/Avatar.ts").default,exports.Badge=__zero_require("./.zero/components/Badge.ts").default,exports.Button=__zero_require("./.zero/components/Button.ts").default,exports.Card=__zero_require("./.zero/components/Card.ts").default,exports.Checkbox=__zero_require("./.zero/components/Checkbox.ts").default,exports.Combobox=__zero_require("./.zero/components/Combobox.ts").default,exports.createForm=__zero_require("./.zero/components/form.ts").createForm;let e=__zero_require("./.zero/components/rules.ts");exports.email=e.email,exports.intRange=e.intRange,exports.maxLength=e.maxLength,exports.minLength=e.minLength,exports.pattern=e.pattern,exports.required=e.required,exports.Dialog=__zero_require("./.zero/components/Dialog.ts").default,exports.Drawer=__zero_require("./.zero/components/Drawer.ts").default,exports.Input=__zero_require("./.zero/components/Input.ts").default,exports.Pagination=__zero_require("./.zero/components/Pagination.ts").default,exports.Radio=__zero_require("./.zero/components/Radio.ts").default,exports.Select=__zero_require("./.zero/components/Select.ts").default,exports.Spinner=__zero_require("./.zero/components/Spinner.ts").default,exports.Tabs=__zero_require("./.zero/components/Tabs.ts").default,exports.Table=__zero_require("./.zero/components/Table.ts").default,exports.TextArea=__zero_require("./.zero/components/TextArea.ts").default,exports.Toast=__zero_require("./.zero/components/Toast.ts").default,exports.Toggle=__zero_require("./.zero/components/Toggle.ts").default;}),__zero_define("./.zero/components/Toggle.ts",function(exports,__zero_require){let{html:e}=__zero_require("zero"),{ariaDescribedBy:t,ariaInvalid:r,debounce:n,errorNode:o,nativeRef:a,uniqueId:s}=__zero_require("./.zero/components/_internal.ts");exports.default=function(l){let i=l.checked,u=n(()=>i.set(!i.val),l.debounceMs??0),c=a(l.attrs,l.autofocus),p=s("toggle-error");return e`<label class="toggle"><input ref=${c} type="checkbox" class="toggle-input" role="switch" checked=${()=>i.val} aria-checked=${()=>String(i.val)} disabled=${l.disabled??!1} aria-invalid=${r(l.error)} aria-describedby=${t(l.error,p)} @change=${u} /><span class="toggle-track"><span class="toggle-thumb"></span></span><span class="toggle-label">${l.label??""}</span></label>${o(l.error,p)}`;};}),__zero_define("./.zero/components/_internal.ts",function(exports,__zero_require){let{html:e}=__zero_require("zero");function t(e){return"object"==typeof e&&null!==e&&"val"in e;}let r=0;exports.nativeRef=function(e,t){if(null==e&&!0!==t)return{el:null};let r=null;return{get el(){return r;},set el(v){if(r=v,null==v)return;Promise.resolve().then(()=>{if(r===v){for(let[t,r]of Object.entries(e??{}))!1===r||v.hasAttribute(t)||v.setAttribute(t,!0===r?"":String(r));!0===t&&v.focus();}});}};},exports.isReactive=t,exports.read=function(e){return t(e)?e.val:e;},exports.uniqueId=function(e){return r+=1,`${e}-${r}`;},exports.errorNode=function(t,r){return e`${()=>t&&null!=t.val?e`<small class="text-muted" id=${r} data-field-error="">${t.val}</small>`:e``}`;},exports.ariaInvalid=function(e){return()=>e?.val!=null?"true":"false";},exports.ariaDescribedBy=function(e,t){return()=>e?.val!=null?t:"";},exports.debounce=function(e,t){if(!(t>0))return e;let r=null;return(...n)=>{null!=r&&clearTimeout(r),r=setTimeout(()=>e(...n),t);};};}),__zero_define("./.zero/components/Toast.ts",function(exports,__zero_require){let{html:e,effect:t}=__zero_require("zero");exports.default=function(r){let n=r.variant??"info",o=`toast toast-${n}`;return null!=r.duration&&t(()=>{if(!r.open.val)return;let e=setTimeout(()=>{r.open.set(!1),r.onDismiss?.();},r.duration);return()=>clearTimeout(e);}),e`${()=>r.open.val?e`<div class=${o} role="status" aria-live="polite">${r.message}</div>`:null}`;};}),__zero_define("./.zero/components/TextArea.ts",function(exports,__zero_require){let{html:e}=__zero_require("zero"),{ariaDescribedBy:t,ariaInvalid:r,debounce:n,errorNode:o,nativeRef:a,uniqueId:s}=__zero_require("./.zero/components/_internal.ts");exports.default=function(l){let i=n(e=>{let t=e.target;l.value.set(t.value);},l.debounceMs??0),u=a(l.attrs,l.autofocus),c=l.label?e`<label class="textarea-label">${l.label}</label>`:null,p=s("textarea-error");return e`${c}<textarea ref=${u} class="textarea" rows=${l.rows??4} placeholder=${l.placeholder??""} disabled=${l.disabled??!1} aria-invalid=${r(l.error)} aria-describedby=${t(l.error,p)} @input=${i}>${()=>l.value.val}</textarea>${o(l.error,p)}`;};}),__zero_define("./.zero/components/Table.ts",function(exports,__zero_require){let{html:e,each:t,computed:r}=__zero_require("zero"),n=__zero_require("./.zero/components/Spinner.ts").default;exports.default=function(o){let a=o.density??"cozy",s="function"==typeof o.onRowClick,l=o.columns.some(e=>null!=e.width),i=["table",`table-${a}`].concat(s?["table-clickable"]:[]).join(" "),u=o.loading,c=u?()=>i+(u.val?" table-loading":""):i,p=o.maxHeight?`max-height: ${o.maxHeight}; overflow-y: auto`:null;if(o.columns.some(e=>!0===e.sortable)&&null==o.sort)throw Error("Table: at least one column has sortable: true but no sort prop was passed. Pass sort: Signal<SortState | null> from the parent.");let d=o.sort,f=e=>{var t;if(!d)return;let r=(t=d.val,null===t||t.key!==e?{key:e,dir:"asc"}:"asc"===t.dir?{key:e,dir:"desc"}:null);d.set(r),o.onSortChange?.(r);},h=o.columns.map(t=>{let r,n;return r="table-th"+(t.align?` table-align-${t.align}`:""),n=t.width?`width: ${t.width}`:null,!0!==t.sortable?e`<th class=${r} style=${n}>${t.label}</th>`:e`<th class=${r} style=${n} aria-sort=${()=>{let e=d?.val;return e&&e.key===t.key?"asc"===e.dir?"ascending":"descending":"none";}}><button type="button" class="button button-ghost button-sm table-sort-btn" @click=${()=>f(t.key)}>${t.label}<span class="table-sort-icon" aria-hidden="true">${()=>{let e=d?.val;return e&&e.key===t.key?"asc"===e.dir?"▲":"▼":"↕";}}</span></button></th>`;}),_=null==o.onSortChange&&null!=d?r(()=>(function(e,t,r){var n;if(null===t)return e;let o=r.find(e=>e.key===t.key);if(!o)return e;let a=o.compare??(n=o.key,(e,t)=>{let r=e[n],o=t[n],a=null==r,s=null==o;return a&&s?0:a?1:s?-1:"number"==typeof r&&"number"==typeof o?r-o:"string"==typeof r&&"string"==typeof o?r.localeCompare(o):String(r).localeCompare(String(o));}),s="desc"===t.dir?-1:1;return[...e].sort((e,t)=>s*a(e,t));})(o.rows.val,d.val,o.columns)):o.rows;return e`<div class=${c} style=${p}><table class=${l?"table-fixed":""}><thead><tr>${h}</tr></thead><tbody>${t(_,(t,r)=>{let n=s?()=>o.onRowClick(t,r):null,a=o.columns.map(n=>{let o="table-td"+(n.align?` table-align-${n.align}`:""),a=n.render?n.render(t,r):t[n.key];return e`<td class=${o}>${a}</td>`;});return e`<tr class="table-row" data-row-index=${r} @click=${n}>${a}</tr>`;},o.rowKey)}${()=>{if(0!==_.val.length)return null;let t=o.empty??e`<span class="text-muted">No data</span>`;return e`<tr class="table-empty"><td colspan=${o.columns.length}>${t}</td></tr>`;}}</tbody></table>${()=>u&&u.val?e`<div class="table-loading-overlay">${n({size:"md"})}</div>`:null}</div>`;};}),__zero_define("./.zero/components/Spinner.ts",function(exports,__zero_require){let{html:e}=__zero_require("zero");exports.default=function(t={}){let r=t.variant??"primary",n=t.size??"md",o=`spinner spinner-${r} spinner-${n}`,a=t.label?e`<span class="visually-hidden">${t.label}</span>`:null;return e`<span class=${o} role="status">${a}</span>`;};}),__zero_define("./.zero/components/Tabs.ts",function(exports,__zero_require){let{html:e}=__zero_require("zero");exports.default=function(t){let r=e=>{let r=(e%t.tabs.length+t.tabs.length)%t.tabs.length;t.active.set(t.tabs[r].id);},n=t.tabs.map(r=>e`<button class="tabs-tab" role="tab" aria-selected=${()=>t.active.val===r.id} @click=${()=>t.active.set(r.id)}>${r.label}</button>`);return e`<div class="tabs"><div class="tabs-list" role="tablist" @keydown=${e=>{let n,o=(n=t.active.val,t.tabs.findIndex(e=>e.id===n));switch(e.key){case"ArrowLeft":r(o-1);break;case"ArrowRight":r(o+1);break;case"Home":r(0);break;case"End":r(t.tabs.length-1);}}}>${n}</div><div class="tabs-panel" role="tabpanel">${()=>t.panels[t.active.val]??null}</div></div>`;};}),__zero_define("./.zero/components/Select.ts",function(exports,__zero_require){let{html:e}=__zero_require("zero"),{ariaDescribedBy:t,ariaInvalid:r,debounce:n,errorNode:o,nativeRef:a,uniqueId:s}=__zero_require("./.zero/components/_internal.ts");exports.default=function(l){let i=l.size??"md",u=`select select-${i}`,c=n(e=>{let t=e.target;l.value.set(t.value),l.onChange?.(t.value);},l.debounceMs??0),p=l.label?e`<label class="select-label">${l.label}</label>`:null,d=l.options.map(t=>e`<option value=${t.value} selected=${()=>l.value.val===t.value}>${t.label}</option>`),f=a(l.attrs,l.autofocus),h=s("select-error");return e`${p}<select ref=${f} class=${u} disabled=${l.disabled??!1} aria-invalid=${r(l.error)} aria-describedby=${t(l.error,h)} @change=${c}>${d}</select>${o(l.error,h)}`;};}),__zero_define("./.zero/components/Radio.ts",function(exports,__zero_require){let{html:e}=__zero_require("zero"),{ariaDescribedBy:t,ariaInvalid:r,debounce:n,errorNode:o,nativeRef:a,uniqueId:s}=__zero_require("./.zero/components/_internal.ts");exports.default=function(l){let i=n(()=>l.selected.set(l.value),l.debounceMs??0),u=a(l.attrs,l.autofocus),c=s("radio-error");return e`<label class="radio"><input ref=${u} type="radio" name=${l.name} value=${l.value} checked=${()=>l.selected.val===l.value} disabled=${l.disabled??!1} aria-invalid=${r(l.error)} aria-describedby=${t(l.error,c)} @change=${i} /><span class="radio-label">${l.label??""}</span></label>${o(l.error,c)}`;};}),__zero_define("./.zero/components/Pagination.ts",function(exports,__zero_require){let{html:e}=__zero_require("zero"),{read:t}=__zero_require("./.zero/components/_internal.ts");function r(e,t){return t<e?[]:Array.from({length:t-e+1},(t,r)=>e+r);}exports.default=function(n){let o=n.size??"md",a=n.siblingCount??1,s=n.boundaryCount??1,l=n.prevLabel??"Previous",i=n.nextLabel??"Next",u=`button button-${o} pagination-btn`,c=`${u} button-ghost`,p=`${u} button-primary pagination-active`,d=()=>Math.max(1,t(n.totalPages)),f=()=>{let e=d(),t=n.page.val;return t<1?1:t>e?e:t;},h=()=>!0===t(n.disabled)||1>=d(),_=e=>{if(h())return;let t=d(),r=e<1?1:e>t?t:e;r!==f()&&(n.page.set(r),n.onChange?.(r));},m=n.summary?()=>e`<div class="pagination-summary text-small">${n.summary(f(),d())}</div>`:null;return e`
    <nav class=${()=>`pagination pagination-${o} stack gap-sm${h()?" pagination-disabled":""}`} role="navigation" aria-label="Pagination">
      ${m}
      <ul class="pagination-list cluster gap-xs">${()=>{let t,n,o,u,m,b=f(),g=d(),$=h(),x=(t=r(1,Math.min(s,g)),n=r(Math.max(g-s+1,s+1),g),o=Math.max(Math.min(b-a,g-s-2*a-1),s+2),u=Math.min(Math.max(b+a,s+2*a+2),n.length>0?n[0]-2:g-1),m=[...t],o>s+2?m.push("..."):s+1<g-s&&m.push(s+1),m.push(...r(o,u)),u<g-s-1?m.push("..."):g-s>s&&m.push(g-s),m.push(...n),m).map(t=>{let r;return"..."===t?e`
    <li><span class="pagination-ellipsis text-muted" aria-hidden="true">…</span></li>
  `:(r=t===b,e`
    <li>
      <button
        class=${r?p:c}
        aria-label=${`Page ${t}`}
        aria-current=${r?"page":null}
        disabled=${$}
        @click=${()=>_(t)}
      >${t}</button>
    </li>
  `);});return[e`
    <li>
      <button
        class=${`${c} pagination-prev`}
        aria-label=${l}
        disabled=${$||b<=1}
        @click=${()=>_(b-1)}
      >‹</button>
    </li>
  `,...x,e`
    <li>
      <button
        class=${`${c} pagination-next`}
        aria-label=${i}
        disabled=${$||b>=g}
        @click=${()=>_(b+1)}
      >›</button>
    </li>
  `];}}</ul>
    </nav>
  `;};}),__zero_define("./.zero/components/Input.ts",function(exports,__zero_require){let{html:e}=__zero_require("zero"),{ariaDescribedBy:t,ariaInvalid:r,debounce:n,errorNode:o,nativeRef:a,uniqueId:s}=__zero_require("./.zero/components/_internal.ts");exports.default=function(l){let i=l.type??"text",u=l.size??"md",c=`input input-${u}`,p=n(e=>{let t=e.target;l.value.set(t.value),l.onChange?.(t.value);},l.debounceMs??0),d=a(l.attrs,l.autofocus),f=l.label?e`<label class="input-label">${l.label}</label>`:null,h=s("input-error");return e`${f}<input ref=${d} class=${c} type=${i} value=${()=>l.value.val} placeholder=${l.placeholder??""} disabled=${l.disabled??!1} aria-invalid=${r(l.error)} aria-describedby=${t(l.error,h)} @input=${p}>${o(l.error,h)}`;};}),__zero_define("./.zero/components/Drawer.ts",function(exports,__zero_require){let{html:e}=__zero_require("zero");exports.default=function(t){let r=t.mode??"overlay",n=t.size??"md",o=t.side,{open:a}=t,s="push"===r?"drawer-push":"drawer-overlay",l=()=>`drawer ${s} drawer-${o} drawer-${n}`+(a.val?" drawer-open":""),i=e=>{let t="function"==typeof e?e():e;return null==t||""===t;},u=e`
    <header class="drawer-title" hidden=${()=>i(t.title)}>${t.title}</header>
    <div class="drawer-body" hidden=${()=>i(t.body)}>${t.body}</div>
    <footer class="drawer-controls" hidden=${()=>i(t.controls)}>${t.controls}</footer>`,c="overlay"===r?e`<div class=${()=>"drawer-backdrop"+(a.val?" drawer-backdrop-open":"")}></div>`:null,p="overlay"===r?e`<aside class=${l} role="dialog" aria-modal="true">${u}</aside>`:e`<aside class=${l} role="complementary">${u}</aside>`;return e`${c}${p}`;};}),__zero_define("./.zero/components/Dialog.ts",function(exports,__zero_require){let{html:e,effect:t}=__zero_require("zero");exports.default=function(r){let n=r.size??"md",o=`dialog dialog-${n} stack pad-lg`,a=()=>{r.open.set(!1),r.onClose?.();};t(()=>{if(!r.open.val)return;let e=e=>{"Escape"===e.key&&a();};return document.addEventListener("keydown",e),()=>document.removeEventListener("keydown",e);});let s=r.title?e`<h2 class="text-h2">${r.title}</h2>`:null;return e`${()=>r.open.val?e`<div class="dialog-backdrop dialog-open" @click=${a}><div class=${o} role="dialog" aria-modal="true" @click.stop=${()=>{}}>${s}<div class="dialog-body">${r.children??""}</div></div></div>`:null}`;};}),__zero_define("./.zero/components/rules.ts",function(exports,__zero_require){function e(e){return"string"==typeof e?{message:e,allowEmpty:!0}:{message:e?.message,allowEmpty:e?.allowEmpty??!0};}function t(e){return""===e.trim();}exports.required=function(e){return r=>t(r)?e??"This field is required.":null;},exports.intRange=function(r,n,o){let{message:a,allowEmpty:s}=e(o),l=`Must be a whole number between ${r} and ${n}.`;return e=>{if(s&&t(e))return null;let o=e.trim();if(!/^[+-]?\d+$/.test(o))return a??l;let i=Number(o);return r<=i&&i<=n?null:a??l;};},exports.email=function(r){let{message:n,allowEmpty:o}=e(r);return e=>o&&t(e)||/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(e.trim())?null:n??"Enter a valid email address.";},exports.pattern=function(r,n){let{message:o,allowEmpty:a}=e(n),s=new RegExp(r.source,r.flags.replace(/[gy]/g,""));return e=>a&&t(e)||s.test(e)?null:o??"Invalid format.";},exports.maxLength=function(r,n){let{message:o,allowEmpty:a}=e(n),s=`Must be ${r} character${1===r?"":"s"} or fewer.`;return e=>a&&t(e)||e.trim().length<=r?null:o??s;},exports.minLength=function(r,n){let{message:o,allowEmpty:a}=e(n),s=`Must be at least ${r} character${1===r?"":"s"}.`;return e=>a&&t(e)||e.trim().length>=r?null:o??s;};}),__zero_define("./.zero/components/form.ts",function(exports,__zero_require){let{computed:e,signal:t}=__zero_require("zero"),{HttpError:r}=__zero_require("zero/http");exports.createForm=function(n){let o=Object.keys(n.fields),a={},s={},l=t(null),i={};for(let e of o){var u;i[e]=null==(u=n.fields[e].validate)?[]:Array.isArray(u)?u:[u];}let c=()=>{let e={};for(let t of o)e[t]=a[t].val;return e;},p=()=>{let e=c(),t={};for(let r of o)for(let n of i[r]){let o=n(e[r],e);if(null!=o){t[r]=o;break;}}if(n.validate){let r=n.validate(e);for(let e of o){let n=r[e];null!=n&&null==t[e]&&(t[e]=n);}}return t;};for(let e of o)s[e]=function(e,r,n,o){let a=t(e.fields[r].initial);n[r]=a;let s=t(null),l=t(!1),i=()=>{l.set(!0),null!=s.val&&s.set(o()[r]??null);};return{value:{get val(){return a.val;},set(e){a.set(e),i();},update(e){a.update(e),i();}},error:s,touched:l};}(n,e,a,p);let d=e(()=>{let e=p();return o.every(t=>null==e[t]);}),f=e=>{for(let t of o)s[t].error.set(e[t]??null);};return{fields:s,isValid:d,error:l,values:c,reset:()=>{for(let e of o)a[e].set(n.fields[e].initial),s[e].error.set(null),s[e].touched.set(!1);l.set(null);},setErrors:f,submit:e=>async t=>{for(let e of(t.preventDefault(),o))s[e].touched.set(!0);let n=p();if(f(n),l.set(null),!o.some(e=>null!=n[e]))try{await e(c());}catch(e){!function(e,t,n,o){if(e instanceof r&&(400===e.status||409===e.status)){let r=e.body,a=null!=r&&"object"==typeof r?r.errors:void 0;if(null!=a&&"object"==typeof a&&!Array.isArray(a)&&Object.keys(a).length>0){let e=new Set(t),r=[];for(let[t,o]of Object.entries(a))e.has(t)?n[t].error.set(String(o)):r.push(String(o));r.length>0&&o.set(r.join(" "));return;}}o.set("Could not save. Try again.");}(e,o,s,l);}}};};}),__zero_define("./.zero/components/Combobox.ts",function(exports,__zero_require){let{html:e,signal:t,effect:r,ref:n}=__zero_require("zero"),{ariaDescribedBy:o,ariaInvalid:a,errorNode:s,nativeRef:l,read:i,uniqueId:u}=__zero_require("./.zero/components/_internal.ts"),c=0;function p(e,t,r){let n=e.inputRef.el;if(null==n)return;let o=t.toLowerCase(),a=r.find(e=>e.label.toLowerCase().startsWith(o));a&&t.length>0?(n.value=a.label,n.setSelectionRange?.(t.length,a.label.length)):n.value=t;}function d(e,t){e.props.value.set(t.value),e.lastLabel.set(t.label),e.highlight.set(-1),e.open.set(!1);let r=e.inputRef.el;null!=r&&(r.value=t.label,r.setSelectionRange?.(t.label.length,t.label.length)),e.props.onChange?.(t.value,t);}function f(e){let t=e.inputRef.el;if(null==t)return;let r=t.value.trim();if(r===e.lastLabel.val){t.value=r,e.open.set(!1),e.highlight.set(-1);return;}let n=r.toLowerCase(),o=e.options.val.find(e=>e.label.toLowerCase()===n);o?d(e,o):(e.props.value.set(r),e.lastLabel.set(r),t.value=r,e.open.set(!1),e.highlight.set(-1),e.props.onChange?.(r,{value:r,label:r}));}function h(e){e.allowCustom?f(e):function(e){let t=e.inputRef.el;if(null!=t){let r=t.value;e.options.val.some(e=>e.label===r)||(t.value=e.lastLabel.val);}e.open.set(!1),e.highlight.set(-1);}(e);}function _(e,t){let r=e.options.val;if(0===r.length)return;!e.open.val&&e.resolved.val&&e.open.set(!0);let n=(e.highlight.val+t+r.length)%r.length;e.highlight.set(n);let o=r[n];o&&p(e,e.query.val,[o]);}function m(){}exports.default=function(b){let g=b.size??"md",$=++c,x=`combobox-input-${$}`,y=`combobox-list-${$}`,w=e=>`combobox-option-${$}-${e}`,z={props:b,debounceMs:b.debounceMs??200,allowCustom:b.allowCustom??!1,minQueryLength:b.minQueryLength??1,noResultsLabel:b.noResultsLabel??"No results",loadingLabel:b.loadingLabel??"Loading…",query:t(""),options:t([]),highlight:t(-1),open:t(!1),busy:t(!1),lastLabel:t(b.initialLabel??""),resolved:t(!1),inputRef:l(b.attrs,b.autofocus),state:{timer:null,serial:0,lastPrefix:"",allowGhost:!1}},k=n();r(()=>{if(!z.open.val)return;let e=e=>{let t=k.el;if(!t)return;let r=e.target;r&&t.contains?.(r)||h(z);};return document.addEventListener("mousedown",e),()=>document.removeEventListener("mousedown",e);}),r(()=>{!0===i(z.props.disabled)&&(z.open.set(!1),z.highlight.set(-1));});let q=b.label?e`<label class="combobox-label" for=${x}>${b.label}</label>`:null,S=u("combobox-error");return e`
    <div
      class=${()=>{let e;return e=`combobox combobox-${g}`,z.open.val&&(e+=" combobox-open"),!0===i(z.props.disabled)&&(e+=" combobox-disabled"),e;}}
      ref=${k}
      role="combobox"
      aria-haspopup="listbox"
      aria-expanded=${()=>z.open.val?"true":"false"}
      aria-owns=${y}
    >
      ${q}
      <div class="combobox-field">
        <input
          ref=${z.inputRef}
          class=${`input input-${g} combobox-input`}
          id=${x}
          type="text"
          role="combobox"
          autocomplete="off"
          aria-autocomplete="both"
          aria-controls=${y}
          aria-activedescendant=${()=>z.highlight.val>=0?w(z.highlight.val):null}
          placeholder=${b.placeholder??""}
          aria-invalid=${a(b.error)}
          aria-describedby=${o(b.error,S)}
          value=${()=>z.lastLabel.val}
          disabled=${()=>!0===i(b.disabled)}
          @input=${e=>(function(e,t){if(!0===i(e.props.disabled))return;let r=t.target,n=r.selectionStart,o=r.value.slice(0,n??r.value.length);e.state.allowGhost=o.length>e.state.lastPrefix.length,e.state.lastPrefix=o,e.query.set(o),function(e,t){if(!0!==i(e.props.disabled)){if(null!=e.state.timer&&clearTimeout(e.state.timer),++e.state.serial,t.length<e.minQueryLength){e.options.set([]),e.busy.set(!1),e.highlight.set(-1),e.open.set(!1);return;}e.state.timer=setTimeout(()=>{var r,n;let o;return r=e,n=t,o=r.state.serial,void(r.busy.set(!0),r.open.set(!0),r.props.loadOptions(n).then(e=>{var t,a,s,l;return t=r,a=n,s=o,l=e,void(s===t.state.serial&&(t.busy.set(!1),t.resolved.set(!0),t.options.set(l),t.highlight.set(l.length>0?0:-1),t.state.allowGhost&&p(t,a,l)));},()=>{var e;o===(e=r).state.serial&&(e.busy.set(!1),e.resolved.set(!0),e.options.set([]),e.highlight.set(-1));}));},e.debounceMs);}}(e,o);})(z,e)}
          @keydown=${e=>(function(e,t){if(!0!==i(e.props.disabled)){let r,n;if("ArrowDown"===t.key)return void(t.preventDefault(),_(e,1));if("ArrowUp"===t.key)return void(t.preventDefault(),_(e,-1));if("Enter"===t.key)return void function(e,t){t.preventDefault();let r=e.options.val[e.highlight.val];if(!e.allowCustom){r&&d(e,r);return;}let n=e.inputRef.el;r&&n&&n.value===r.label?d(e,r):f(e);}(e,t);if("Escape"===t.key)return void(t.preventDefault(),e.open.set(!1),e.highlight.set(-1));"Tab"===t.key&&(r=e.options.val[e.highlight.val],n=e.inputRef.el,r&&n&&n.value===r.label?(t.preventDefault(),d(e,r)):e.open.set(!1));}})(z,e)}
          @focus=${()=>{!0!==i(z.props.disabled)&&z.resolved.val&&z.options.val.length>0&&z.open.set(!0);}}
          @blur=${()=>h(z)}
        >
        <span class="combobox-spinner" hidden=${()=>!z.busy.val} aria-hidden="true"></span>
      </div>
      <ul
        class="combobox-list border pad-0"
        id=${y}
        role="listbox"
        hidden=${()=>!z.open.val}
        aria-busy=${()=>z.busy.val?"true":"false"}
      >${()=>(function(t,r){return t.busy.val&&0===t.options.val.length?e`<li class="combobox-loading" aria-busy="true">${t.loadingLabel}</li>`:t.resolved.val&&0===t.options.val.length?e`<li class="combobox-empty" aria-disabled="true">${t.noResultsLabel}</li>`:e`${t.options.val.map((n,o)=>e`
      <li
        class=${()=>"combobox-option"+(t.highlight.val===o?" combobox-option-active":"")}
        id=${r(o)}
        role="option"
        aria-selected=${()=>t.highlight.val===o?"true":"false"}
        @mousedown.prevent=${m}
        @click=${()=>d(t,n)}
      >${n.label}</li>
    `)}`;})(z,w)}</ul>
    </div>${s(b.error,S)}
  `;};}),__zero_define("./.zero/components/Checkbox.ts",function(exports,__zero_require){let{html:e}=__zero_require("zero"),{ariaDescribedBy:t,ariaInvalid:r,debounce:n,errorNode:o,nativeRef:a,uniqueId:s}=__zero_require("./.zero/components/_internal.ts");exports.default=function(l){let i=l.checked,u=n(()=>i.set(!i.val),l.debounceMs??0),c=a(l.attrs,l.autofocus),p=s("checkbox-error");return e`<label class="checkbox"><input ref=${c} type="checkbox" checked=${()=>i.val} disabled=${l.disabled??!1} aria-invalid=${r(l.error)} aria-describedby=${t(l.error,p)} @change=${u} /><span class="checkbox-label">${l.label??""}</span></label>${o(l.error,p)}`;};}),__zero_define("./.zero/components/Card.ts",function(exports,__zero_require){let{html:e}=__zero_require("zero");exports.default=function(t={}){let r=t.variant??"surface",n=`card card-${r}`,o=t.title?e`<h3 class="card-title">${t.title}</h3>`:null;return e`<section class=${n}>${o}<div class="card-body">${t.children??""}</div></section>`;};}),__zero_define("./.zero/components/Button.ts",function(exports,__zero_require){let{html:e}=__zero_require("zero");exports.default=function(t={}){let r=t.variant??"primary",n=t.size??"md",o=t.type??"button",a=`button button-${r} button-${n}`,s=`button-spinner spinner spinner-${r} spinner-sm`,l=t.loading?e`<span class=${s} role="status" aria-label="loading"></span>`:null,i=(t.disabled??!1)||(t.loading??!1);return e`<button class=${a} type=${o} form=${t.form} name=${t.name} value=${t.value} disabled=${i} @click=${e=>{i||t.onClick?.(e);}}>${l}${t.children??""}</button>`;};}),__zero_define("./.zero/components/Badge.ts",function(exports,__zero_require){let{html:e}=__zero_require("zero");exports.default=function(t={}){let r=t.variant??"default",n=t.size??"md",o=`badge badge-${r} badge-${n}`;return e`<span class=${o}>${t.children??""}</span>`;};}),__zero_define("./.zero/components/Avatar.ts",function(exports,__zero_require){let{html:e}=__zero_require("zero");exports.default=function(t){let r=t.size??"md";if(t.src){let n=`avatar avatar-${r}`;return e`<img class=${n} src=${t.src} alt=${t.alt}>`;}let n=`avatar avatar-${r} avatar-initials`,o=t.initials??t.alt[0]?.toUpperCase()??"";return e`<span class=${n} aria-label=${t.alt}>${o}</span>`;};}),__zero_define("./src/routes/live-log.ts",function(exports,__zero_require){let{computed:e,each:t,effect:r,html:n,ref:o,signal:a}=__zero_require("zero"),{Input:s,Select:l}=__zero_require("./.zero/components/index.ts"),{clearEvents:i}=__zero_require("./src/lib/api.ts"),{bytesCell:u,statusClass:c,targetOf:p}=__zero_require("./src/lib/format.ts"),{PauseIcon:d,PlayIcon:f,TrashIcon:h}=__zero_require("./src/components/icons.ts"),{locationToUrl:_,parentPrefix:m}=__zero_require("./src/lib/browse.ts"),{appendCapped:b,matchesFilter:g,timeAgo:$}=__zero_require("./src/lib/log.ts");exports.default=function(){var x,y,w,z,k,q;let S,T,A,E,C,j,L,N,R,M,I,B,O,D,P,U,H,W=(S=a([]),T=a(!1),A=a(0),E=a(""),C=a("all"),j=a("any"),L=a(null),N=o(),R=a(Date.now()),r(()=>{let e=setInterval(()=>R.set(Date.now()),1e3);return()=>clearInterval(e);}),U=(x=S,y=T,w=A,z=N,M=!0,I=[],B=!1,O=()=>{I=[],x.set([]),w.set(0);},D=e=>{var t;if(0===e.length)return;let r=z.el,n=r?.scrollHeight??0,o=r?.scrollTop??0;x.update(t=>b(t,e,2e3)),r&&(t=M&&!y.val,r.scrollTop=t?0:o+(r.scrollHeight-n));},P=()=>{if(B=!1,0===I.length)return;if(y.val)return void w.set(I.length);let e=I;I=[],D(e);},k=e=>{try{let t=JSON.parse(e);if(!0===t.clear)return void O();"number"==typeof t.id&&(I.push(t),B||(B=!0,requestAnimationFrame(P)));}catch{}},r(()=>{let e=new EventSource("/_/api/events");return e.onmessage=e=>k(e.data),()=>e.close();}),{clear:O,resume:()=>{y.set(!1),w.set(0);let e=I;I=[],M=!0,D(e);},onScroll:()=>{let e=z.el;e&&(M=e.scrollTop<=8);}}),H=e(()=>S.val.filter(e=>g(e,E.val,C.val,j.val)).reverse()),{events:S,visible:H,paused:T,newCount:A,filter:E,statusFilter:C,authFilter:j,expanded:L,scroller:N,now:R,resume:U.resume,clear:U.clear,onScroll:U.onScroll});return n`
    <section class="screen log-screen stack gap-0">
      ${function(e){let{filter:t,statusFilter:r,authFilter:o,visible:a,events:u,paused:c,newCount:p,resume:_,clear:m}=e;return n`
    <div class="toolbar split align-center pad-md border-b">
      <div class="cluster align-center gap-md">
        <div class="toolbar-filter">
          ${s({value:t,placeholder:"Filter by op, key, method",size:"sm"})}
        </div>
        ${l({value:r,options:[{value:"all",label:"All status"},{value:"2",label:"2xx"},{value:"3",label:"3xx"},{value:"4",label:"4xx"},{value:"5",label:"5xx"}],size:"sm"})}
        ${l({value:o,options:[{value:"any",label:"Any auth"},{value:"header",label:"Header"},{value:"presigned",label:"Presigned"},{value:"anonymous",label:"Anonymous"}],size:"sm"})}
      </div>
      <div class="cluster align-center gap-md">
        <span class="count mono">${()=>`${a.val.length} / ${u.val.length}`}</span>
        <button class="clear-btn cluster align-center" @click=${()=>{i(),m();}} title="Clear log" aria-label="Clear log">${h()}</button>
        <button
          class=${()=>"pause-btn cluster align-center gap-xs"+(c.val?" paused":"")}
          @click=${()=>{c.val?_():c.set(!0);}}
          title=${()=>c.val?"Resume live tail":"Pause live tail"}
          aria-label=${()=>c.val?"Resume live tail":"Pause live tail"}
        >
          ${()=>c.val?f():d()}
          ${()=>c.val&&p.val>0?n`<span class="pause-count mono">${p}</span>`:""}
        </button>
      </div>
    </div>
  `;}(W)}
      <div class="log-wrap" ref=${W.scroller} @scroll=${W.onScroll}>
        ${q=W,n`
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
        ${t(q.visible,e=>{var t,r,o;let a;return t=e,r=q.expanded,o=q.now,a="c-dur"+(t.duration_ms>=100?" slow":""),n`
    <tr class="log-row" @click=${()=>r.update(e=>e===t.id?null:t.id)}>
      <td class="c-time mono">${()=>$(t.ts,o.val)}</td>
      <td class="c-method"><span class=${"method m-"+t.method.toLowerCase()}>${t.method}</span></td>
      <td class="c-op">${t.op??"—"}</td>
      <td class="c-key mono" title=${p(t)}>${p(t)}</td>
      <td class="c-status">
        <span class=${"pill s-"+c(t.status)}>${t.status}</span>
      </td>
      <td class=${a+" mono"}>${t.duration_ms} ms</td>
      <td class="c-bytes mono">${u(t)}</td>
    </tr>
    <tr class=${()=>"log-detail"+(r.val===t.id?" open":"")}>
      ${()=>{var e;let o;return r.val===t.id?(e=t,o=(e,t)=>n`<div class="kv"><span class="k">${e}</span><span class="v mono">${t}</span></div>`,n`
    <td colspan="7">
      <div class="detail-grid grid">
        ${o("time",new Date(e.ts).toISOString())}
        ${o("op",e.op??"—")}
        ${o("auth",e.auth)}
        ${o("error_code",e.error_code??"—")}
        ${o("bytes_in",String(e.bytes_in))}
        ${o("bytes_out",String(e.bytes_out))}
        ${o("duration",e.duration_ms+" ms")}
        ${o("id",String(e.id))}
      </div>
      ${function(e){if(!e.bucket||!e.key)return"";let t=_({bucket:e.bucket,prefix:m(e.key),object:e.key});return n`<a class="detail-jump" href=${t}>View object →</a>`;}(e)}
    </td>
  `):"";}}
    </tr>
  `;},e=>e.id)}
      </tbody>
    </table>
  `}
        ${()=>0===W.visible.val.length?n`<div class="empty-state text-center">Waiting for S3 traffic…</div>`:""}
      </div>
    </section>
  `;};}),__zero_define("./src/lib/log.ts",function(exports,__zero_require){let{targetOf:e}=__zero_require("./src/lib/format.ts");exports.matchesFilter=function(t,r,n,o){if("all"!==n&&Math.floor(t.status/100)!==Number(n)||"any"!==o&&t.auth!==o)return!1;let a=r.trim().toLowerCase();return!a||`${t.method} ${t.op??""} ${e(t)}`.toLowerCase().includes(a);},exports.appendCapped=function(e,t,r){if(0===t.length)return e;let n=e.concat(t);return n.length>r?n.slice(n.length-r):n;},exports.timeAgo=function(e,t){let r=Math.max(0,Math.floor((t-e)/1e3));if(r<1)return"now";if(r<60)return`${r}s`;let n=Math.floor(r/60);if(n<60)return`${n}m`;let o=Math.floor(n/60);return o<24?`${o}h`:`${Math.floor(o/24)}d`;};}),__zero_define("./src/components/chrome.ts",function(exports,__zero_require){let{html:e,route:t}=__zero_require("zero"),{middleTruncate:r}=__zero_require("./src/lib/format.ts"),{MoonIcon:n,SunIcon:o,SystemIcon:a}=__zero_require("./src/components/icons.ts"),{cycleTheme:s,health:l,healthy:i,themePref:u}=__zero_require("./src/stores/chrome.ts"),c={dark:{icon:n,label:"Dark"},light:{icon:o,label:"Light"},system:{icon:a,label:"System"}};exports.default=function(n){let o,a;return e`
    <div class="app-shell">
      ${e`
    <header class="topbar split align-center pad-md border-b">
      <div class="cluster align-center gap-lg">
        <div class="cluster align-center gap-sm">
          <span class="brand-mark" aria-hidden="true">
            <svg viewBox="0 0 16 16">
              <path fill="currentColor" d="M8 1 14 4.5 8 8 2 4.5Z"></path>
              <path fill="currentColor" opacity="0.78" d="M2 4.5 8 8 8 15 2 11.5Z"></path>
              <path fill="currentColor" opacity="0.55" d="M14 4.5 14 11.5 8 15 8 8Z"></path>
            </svg>
          </span>
          <span class="brand-name text-h4">cubby</span>
          <span class="badge-version mono">${()=>"v"+(l.val?.version??"…")}</span>
        </div>
        <div class="cluster align-center gap-xs">
          <span class="chrome-label">DATA-DIR</span>
          <span class="chrome-value mono" title=${()=>l.val?.data_dir??""}
            >${()=>r(l.val?.data_dir??"…",50)}</span>
        </div>
      </div>
      <div class="cluster align-center gap-md">
        <span class="cluster align-center gap-xs">
          <span class=${()=>"status-dot "+(i.val?"ok":"down")}></span>
          <span class="status-text">${()=>i.val?"healthy":"offline"}</span>
        </span>
        <button
          class="theme-toggle cluster align-center justify-center"
          @click=${s}
          aria-label=${()=>`Theme: ${c[u.val].label} (click to change)`}
          title=${()=>`Theme: ${c[u.val].label}`}
        >
          ${()=>c[u.val].icon()}
        </button>
      </div>
    </header>
  `}
      <div class="app-body flank gap-0">
        ${o=t(),a="nav-item split align-center",e`
    <nav class="nav stack justify-between border-r pad-md">
      <div class="stack gap-xs">
        <div class="nav-heading">INSPECT</div>
        <a class=${()=>a+("/_"===o.path||"/_/"===o.path?" active":"")} href="/_/">
          <span>Live request log</span>
          <span class="live-dot" aria-hidden="true"></span>
        </a>
        <a class=${()=>a+(o.path.startsWith("/_/browser")?" active":"")} href="/_/browser">Bucket browser</a>
      </div>
      <div class="nav-footer stack gap-xs">
        <div class="mono">
          <b>${()=>l.val?.bucket_count??0}</b> buckets ·
          <b>${()=>l.val?.object_count??0}</b> objects
        </div>
      </div>
    </nav>
  `}
        <main class="app-main stack gap-0">${n.outlet}</main>
      </div>
    </div>
  `;};}),__zero_require("./src/app.ts");